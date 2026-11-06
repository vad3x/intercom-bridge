#include "dtls_intercom.h"

#include <mbedtls/ctr_drbg.h>
#include <mbedtls/debug.h>
#include <mbedtls/ecp.h>
#include <mbedtls/md.h>
#include <mbedtls/memory_buffer_alloc.h>
#include <mbedtls/pk.h>
#include <mbedtls/platform.h>
#include <mbedtls/sha256.h>
#include <mbedtls/ssl.h>
#include <mbedtls/ssl_cookie.h>
#include <mbedtls/ssl_ciphersuites.h>
#include <mbedtls/x509_crt.h>

#include <stdbool.h>
#include <string.h>

typedef struct {
    mbedtls_ssl_context ssl;
    mbedtls_ssl_config config;
    mbedtls_ctr_drbg_context ctr_drbg;
    mbedtls_ssl_cookie_ctx cookie;
    mbedtls_x509_crt local_cert;
    mbedtls_pk_context local_key;
    mbedtls_ssl_srtp_profile srtp_profiles[2];
    uint8_t seed[32];
    uint32_t entropy_counter;
    uint64_t now_ms;
    uint64_t timer_start_ms;
    uint32_t timer_intermediate_ms;
    uint32_t timer_final_ms;
    uint8_t input[INTERCOM_DTLS_DATAGRAM_MAX];
    size_t input_length;
    bool input_pending;
    unsigned char *client_hello_fragment_data;
    uint8_t client_hello_fragment_map[
        (INTERCOM_DTLS_DATAGRAM_MAX - 25 + 7) / 8];
    size_t client_hello_fragment_total;
    size_t client_hello_fragment_received;
    uint16_t client_hello_fragment_seq;
    uint8_t client_hello_record_header[13];
    bool client_hello_fragment_active;
    uint8_t output[INTERCOM_DTLS_DATAGRAM_MAX];
    size_t output_length;
    bool output_pending;
    uint8_t exported_keys[60];
    bool keys_ready;
    int export_error;
    bool export_callback_called;
    int export_type;
    int export_prf;
    int debug_level;
    int debug_line;
    char debug_message[128];
    uint8_t local_fingerprint[32];
    uint8_t peer_fingerprint[32];
    bool peer_fingerprint_ready;
    bool initialized;
    bool handshake_complete;
    bool is_server;
    bool cookie_initialized;
    uint8_t peer_transport_id[32];
    size_t peer_transport_id_length;
} intercom_dtls_state;

static const int intercom_cipher_suites[] = {
    MBEDTLS_TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
    0
};

static void intercom_debug(void *context, int level, const char *file,
                           int line, const char *message)
{
    (void) file;
    intercom_dtls_state *state = (intercom_dtls_state *) context;
    state->debug_level = level;
    state->debug_line = line;
    size_t length = strlen(message);
    if (length >= sizeof(state->debug_message)) {
        length = sizeof(state->debug_message) - 1;
    }
    memcpy(state->debug_message, message, length);
    state->debug_message[length] = '\0';
}

_Static_assert(sizeof(intercom_dtls_state) <= INTERCOM_DTLS_STATE_BYTES,
               "increase the Rust opaque DTLS state storage");

static bool g_heap_initialized;

#define INTERCOM_DTLS_RECORD_HEADER_BYTES 13
#define INTERCOM_DTLS_HANDSHAKE_HEADER_BYTES 12
#define INTERCOM_DTLS_CLIENT_HELLO_CAPACITY \
    (INTERCOM_DTLS_DATAGRAM_MAX - INTERCOM_DTLS_RECORD_HEADER_BYTES - \
     INTERCOM_DTLS_HANDSHAKE_HEADER_BYTES)

int intercom_mbedtls_printf(const char *format, ...)
{
    (void) format;
    return 0;
}

static int intercom_seed_entropy(void *context, unsigned char *output,
                                 size_t length)
{
    intercom_dtls_state *state = (intercom_dtls_state *) context;
    uint8_t input[36];
    uint8_t digest[32];
    size_t copied = 0;

    memcpy(input, state->seed, sizeof(state->seed));
    while (copied < length) {
        const uint32_t counter = state->entropy_counter++;
        input[32] = (uint8_t) (counter >> 24);
        input[33] = (uint8_t) (counter >> 16);
        input[34] = (uint8_t) (counter >> 8);
        input[35] = (uint8_t) counter;
        if (mbedtls_sha256(input, sizeof(input), digest, 0) != 0) {
            return -1;
        }
        size_t chunk_length = length - copied;
        if (chunk_length > sizeof(digest)) {
            chunk_length = sizeof(digest);
        }
        memcpy(output + copied, digest, chunk_length);
        copied += chunk_length;
    }
    return 0;
}
static int intercom_bio_send(void *context, const unsigned char *data, size_t length)
{
    intercom_dtls_state *state = (intercom_dtls_state *) context;
    if (state->output_pending) {
        return MBEDTLS_ERR_SSL_WANT_WRITE;
    }
    if (length > sizeof(state->output)) {
        return MBEDTLS_ERR_SSL_BUFFER_TOO_SMALL;
    }
    memcpy(state->output, data, length);
    state->output_length = length;
    state->output_pending = true;
    return (int) length;
}

static int intercom_bio_recv(void *context, unsigned char *output, size_t capacity)
{
    intercom_dtls_state *state = (intercom_dtls_state *) context;
    if (!state->input_pending) {
        return MBEDTLS_ERR_SSL_WANT_READ;
    }
    if (state->input_length > capacity) {
        state->input_pending = false;
        state->input_length = 0;
        return MBEDTLS_ERR_SSL_BUFFER_TOO_SMALL;
    }
    memcpy(output, state->input, state->input_length);
    const size_t length = state->input_length;
    state->input_pending = false;
    state->input_length = 0;
    return (int) length;
}

static size_t intercom_read_u24(const unsigned char *input)
{
    return ((size_t) input[0] << 16) | ((size_t) input[1] << 8) | input[2];
}

static void intercom_write_u24(unsigned char *output, size_t value)
{
    output[0] = (unsigned char) (value >> 16);
    output[1] = (unsigned char) (value >> 8);
    output[2] = (unsigned char) value;
}

static void intercom_clear_client_hello_reassembly(intercom_dtls_state *state)
{
    memset(state->client_hello_fragment_map, 0,
           sizeof(state->client_hello_fragment_map));
    state->client_hello_fragment_total = 0;
    state->client_hello_fragment_received = 0;
    state->client_hello_fragment_seq = 0;
    state->client_hello_fragment_active = false;
}

/*
 * Reassemble fragmented DTLS ClientHello messages before they reach Mbed TLS,
 * whose TLS 1.2 server parser only accepts an unfragmented ClientHello.
 * Returns 0 for a normal datagram, 1 when a fragment was consumed, or an
 * Mbed TLS error for malformed/oversized fragments.
 */
static int intercom_reassemble_client_hello(intercom_dtls_state *state,
                                            const unsigned char *input,
                                            size_t input_length)
{
    if (input_length < INTERCOM_DTLS_RECORD_HEADER_BYTES +
                           INTERCOM_DTLS_HANDSHAKE_HEADER_BYTES ||
        input[0] != MBEDTLS_SSL_MSG_HANDSHAKE ||
        input[INTERCOM_DTLS_RECORD_HEADER_BYTES] != MBEDTLS_SSL_HS_CLIENT_HELLO) {
        return 0;
    }

    const size_t record_length = ((size_t) input[11] << 8) | input[12];
    const size_t total = intercom_read_u24(input + 14);
    const uint16_t message_seq = (uint16_t) (((uint16_t) input[17] << 8) | input[18]);
    const size_t offset = intercom_read_u24(input + 19);
    const size_t fragment_length = intercom_read_u24(input + 22);
    const size_t fragment_start = INTERCOM_DTLS_RECORD_HEADER_BYTES +
                                  INTERCOM_DTLS_HANDSHAKE_HEADER_BYTES;

    if (record_length != input_length - INTERCOM_DTLS_RECORD_HEADER_BYTES ||
        fragment_length != input_length - fragment_start || total == 0 ||
        total > INTERCOM_DTLS_CLIENT_HELLO_CAPACITY || offset > total ||
        fragment_length > total - offset) {
        return MBEDTLS_ERR_SSL_DECODE_ERROR;
    }
    if (offset == 0 && fragment_length == total) {
        return 0;
    }
    if (state->client_hello_fragment_data == NULL) {
        return MBEDTLS_ERR_SSL_ALLOC_FAILED;
    }

    if (!state->client_hello_fragment_active ||
        state->client_hello_fragment_seq != message_seq ||
        state->client_hello_fragment_total != total) {
        intercom_clear_client_hello_reassembly(state);
        state->client_hello_fragment_active = true;
        state->client_hello_fragment_seq = message_seq;
        state->client_hello_fragment_total = total;
    }
    if (offset == 0) {
        memcpy(state->client_hello_record_header, input,
               INTERCOM_DTLS_RECORD_HEADER_BYTES);
    }

    for (size_t i = 0; i < fragment_length; i++) {
        const size_t index = offset + i;
        const uint8_t mask = (uint8_t) (1u << (index & 7));
        uint8_t *bitmap_byte = &state->client_hello_fragment_map[index >> 3];
        if ((*bitmap_byte & mask) != 0) {
            if (state->client_hello_fragment_data[index] != input[fragment_start + i]) {
                intercom_clear_client_hello_reassembly(state);
                return MBEDTLS_ERR_SSL_DECODE_ERROR;
            }
        } else {
            state->client_hello_fragment_data[index] = input[fragment_start + i];
            *bitmap_byte |= mask;
            state->client_hello_fragment_received++;
        }
    }

    if (state->client_hello_fragment_received != total) {
        return 1;
    }

    memcpy(state->input, state->client_hello_record_header,
           INTERCOM_DTLS_RECORD_HEADER_BYTES);
    state->input[13] = MBEDTLS_SSL_HS_CLIENT_HELLO;
    intercom_write_u24(state->input + 14, total);
    state->input[17] = (unsigned char) (message_seq >> 8);
    state->input[18] = (unsigned char) message_seq;
    intercom_write_u24(state->input + 19, 0);
    intercom_write_u24(state->input + 22, total);
    memcpy(state->input + fragment_start, state->client_hello_fragment_data, total);
    const size_t reassembled_record_length =
        INTERCOM_DTLS_HANDSHAKE_HEADER_BYTES + total;
    state->input[11] = (unsigned char) (reassembled_record_length >> 8);
    state->input[12] = (unsigned char) reassembled_record_length;
    state->input_length = fragment_start + total;
    state->input_pending = true;
    intercom_clear_client_hello_reassembly(state);
    return 1;
}

static void intercom_timer_set(void *context, uint32_t intermediate_ms, uint32_t final_ms)
{
    intercom_dtls_state *state = (intercom_dtls_state *) context;
    state->timer_start_ms = state->now_ms;
    state->timer_intermediate_ms = intermediate_ms;
    state->timer_final_ms = final_ms;
}

static int intercom_timer_get(void *context)
{
    intercom_dtls_state *state = (intercom_dtls_state *) context;
    if (state->timer_final_ms == 0) {
        return -1;
    }
    const uint64_t elapsed = state->now_ms - state->timer_start_ms;
    if (elapsed >= state->timer_final_ms) {
        return 2;
    }
    if (elapsed >= state->timer_intermediate_ms) {
        return 1;
    }
    return 0;
}

static void intercom_export_srtp_keys(void *context,
                                      mbedtls_ssl_key_export_type type,
                                      const unsigned char *secret,
                                      size_t secret_length,
                                      const unsigned char client_random[32],
                                      const unsigned char server_random[32],
                                      mbedtls_tls_prf_types prf)
{
    intercom_dtls_state *state = (intercom_dtls_state *) context;
    state->export_callback_called = true;
    state->export_type = (int) type;
    state->export_prf = (int) prf;
    if (type != MBEDTLS_SSL_KEY_EXPORT_TLS12_MASTER_SECRET) {
        state->export_error = MBEDTLS_ERR_SSL_FEATURE_UNAVAILABLE;
        return;
    }

    uint8_t random[64];
    memcpy(random, client_random, 32);
    memcpy(random + 32, server_random, 32);
    state->export_error = mbedtls_ssl_tls_prf(prf, secret, secret_length,
                                              "EXTRACTOR-dtls_srtp", random,
                                              sizeof(random), state->exported_keys,
                                              sizeof(state->exported_keys));
    state->keys_ready = state->export_error == 0;
}

static int intercom_make_self_signed_certificate(intercom_dtls_state *state)
{
    int result;
    mbedtls_x509write_cert writer;
    uint8_t serial[16];
    memset(&writer, 0, sizeof(writer));
    memcpy(serial, state->seed, sizeof(serial));
    serial[0] |= 0x01;

    if ((result = mbedtls_pk_setup(&state->local_key,
                                   mbedtls_pk_info_from_type(MBEDTLS_PK_ECKEY))) != 0) {
        return result;
    }
    if ((result = mbedtls_ecp_gen_key(MBEDTLS_ECP_DP_SECP256R1,
                                      mbedtls_pk_ec(state->local_key),
                                      mbedtls_ctr_drbg_random,
                                      &state->ctr_drbg)) != 0) {
        return result;
    }

    // P-256 certificate generation already has a deep call stack on the C6.
    // Keep the DER scratch space in the bounded Mbed TLS arena instead of
    // consuming another 2 KiB of the Embassy main task stack.
    uint8_t *der = mbedtls_calloc(1, 2048);
    if (der == NULL) {
        return MBEDTLS_ERR_X509_ALLOC_FAILED;
    }

    mbedtls_x509write_crt_init(&writer);
    mbedtls_x509write_crt_set_version(&writer, MBEDTLS_X509_CRT_VERSION_3);
    mbedtls_x509write_crt_set_md_alg(&writer, MBEDTLS_MD_SHA256);
    mbedtls_x509write_crt_set_subject_key(&writer, &state->local_key);
    mbedtls_x509write_crt_set_issuer_key(&writer, &state->local_key);
    if ((result = mbedtls_x509write_crt_set_subject_name(&writer, "CN=Intercom Audio")) != 0 ||
        (result = mbedtls_x509write_crt_set_issuer_name(&writer, "CN=Intercom Audio")) != 0 ||
        (result = mbedtls_x509write_crt_set_serial_raw(&writer, serial, sizeof(serial))) != 0 ||
        (result = mbedtls_x509write_crt_set_validity(&writer, "20260101000000", "20360101000000")) != 0 ||
        (result = mbedtls_x509write_crt_set_basic_constraints(&writer, 0, -1)) != 0) {
        mbedtls_x509write_crt_free(&writer);
        mbedtls_free(der);
        return result;
    }

    const int der_length = mbedtls_x509write_crt_der(&writer, der, 2048,
                                                     mbedtls_ctr_drbg_random,
                                                     &state->ctr_drbg);
    mbedtls_x509write_crt_free(&writer);
    if (der_length <= 0) {
        mbedtls_free(der);
        return der_length;
    }
    const unsigned char *der_start = der + 2048 - (size_t) der_length;
    if ((result = mbedtls_x509_crt_parse_der(&state->local_cert, der_start,
                                             (size_t) der_length)) != 0) {
        mbedtls_free(der);
        return result;
    }
    result = mbedtls_sha256(der_start, (size_t) der_length,
                            state->local_fingerprint, 0);
    mbedtls_free(der);
    return result;
}

int intercom_dtls_heap_init(unsigned char *buffer, size_t length)
{
    if (buffer == NULL || length < 4096 || g_heap_initialized) {
        return -1;
    }
    mbedtls_memory_buffer_alloc_init(buffer, length);
    g_heap_initialized = true;
    return 0;
}

void intercom_dtls_heap_free(void)
{
    if (!g_heap_initialized) {
        return;
    }
    mbedtls_memory_buffer_alloc_free();
    g_heap_initialized = false;
}

size_t intercom_dtls_state_size(void)
{
    return sizeof(intercom_dtls_state);
}

int intercom_dtls_init(void *storage, size_t storage_length, int role,
                       const unsigned char seed[32])
{
    if (storage == NULL || storage_length < sizeof(intercom_dtls_state) ||
        seed == NULL || !g_heap_initialized ||
        (role != MBEDTLS_SSL_IS_CLIENT && role != MBEDTLS_SSL_IS_SERVER)) {
        return MBEDTLS_ERR_SSL_BAD_INPUT_DATA;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    memset(state, 0, sizeof(*state));
    state->is_server = role == MBEDTLS_SSL_IS_SERVER;
    memcpy(state->seed, seed, sizeof(state->seed));
    state->client_hello_fragment_data = mbedtls_calloc(
        1, INTERCOM_DTLS_CLIENT_HELLO_CAPACITY);
    if (state->client_hello_fragment_data == NULL) {
        return MBEDTLS_ERR_SSL_ALLOC_FAILED;
    }
    mbedtls_ssl_init(&state->ssl);
    mbedtls_ssl_config_init(&state->config);
    mbedtls_ctr_drbg_init(&state->ctr_drbg);
    mbedtls_ssl_cookie_init(&state->cookie);
    state->cookie_initialized = true;
    mbedtls_x509_crt_init(&state->local_cert);
    mbedtls_pk_init(&state->local_key);

    int result = mbedtls_ctr_drbg_seed(&state->ctr_drbg, intercom_seed_entropy,
                                       state, state->seed, sizeof(state->seed));
    if (result != 0) {
        goto fail;
    }
    result = mbedtls_ssl_cookie_setup(&state->cookie, mbedtls_ctr_drbg_random,
                                     &state->ctr_drbg);
    if (result != 0) {
        goto fail;
    }
    result = intercom_make_self_signed_certificate(state);
    if (result != 0) {
        goto fail;
    }
    result = mbedtls_ssl_config_defaults(&state->config, role,
                                        MBEDTLS_SSL_TRANSPORT_DATAGRAM,
                                        MBEDTLS_SSL_PRESET_DEFAULT);
    if (result != 0) {
        goto fail;
    }
    mbedtls_debug_set_threshold(1);
    mbedtls_ssl_conf_dbg(&state->config, intercom_debug, state);
    mbedtls_ssl_conf_dtls_cookies(&state->config, mbedtls_ssl_cookie_write,
                                  mbedtls_ssl_cookie_check, &state->cookie);
    mbedtls_ssl_conf_rng(&state->config, mbedtls_ctr_drbg_random, &state->ctr_drbg);
    mbedtls_ssl_conf_authmode(&state->config, MBEDTLS_SSL_VERIFY_OPTIONAL);
    mbedtls_ssl_conf_legacy_renegotiation(&state->config,
                                           MBEDTLS_SSL_LEGACY_NO_RENEGOTIATION);
    mbedtls_ssl_conf_handshake_timeout(&state->config, 1000, 8000);
    mbedtls_ssl_conf_ciphersuites(&state->config, intercom_cipher_suites);
    state->srtp_profiles[0] = MBEDTLS_TLS_SRTP_AES128_CM_HMAC_SHA1_80;
    state->srtp_profiles[1] = MBEDTLS_TLS_SRTP_UNSET;
    result = mbedtls_ssl_conf_dtls_srtp_protection_profiles(&state->config,
                                                            state->srtp_profiles);
    if (result != 0) {
        goto fail;
    }
    result = mbedtls_ssl_conf_own_cert(&state->config, &state->local_cert,
                                       &state->local_key);
    if (result != 0) {
        goto fail;
    }
    result = mbedtls_ssl_setup(&state->ssl, &state->config);
    if (result != 0) {
        goto fail;
    }
    mbedtls_ssl_set_bio(&state->ssl, state, intercom_bio_send, intercom_bio_recv, NULL);
    mbedtls_ssl_set_timer_cb(&state->ssl, state, intercom_timer_set, intercom_timer_get);
    mbedtls_ssl_set_export_keys_cb(&state->ssl, intercom_export_srtp_keys, state);
    mbedtls_ssl_set_mtu(&state->ssl, 1200);
    if (role == MBEDTLS_SSL_IS_SERVER && state->peer_transport_id_length != 0) {
        result = mbedtls_ssl_set_client_transport_id(&state->ssl,
                                                     state->peer_transport_id,
                                                     state->peer_transport_id_length);
        if (result != 0) {
            goto fail;
        }
    }
    state->initialized = true;
    return 0;

fail:
    mbedtls_ssl_free(&state->ssl);
    mbedtls_ssl_config_free(&state->config);
    mbedtls_ctr_drbg_free(&state->ctr_drbg);
    if (state->cookie_initialized) {
        mbedtls_ssl_cookie_free(&state->cookie);
    }
    mbedtls_x509_crt_free(&state->local_cert);
    mbedtls_pk_free(&state->local_key);
    mbedtls_free(state->client_hello_fragment_data);
    memset(state, 0, sizeof(*state));
    return result;
}

void intercom_dtls_free(void *storage)
{
    if (storage == NULL) {
        return;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    if (state->initialized) {
        mbedtls_ssl_free(&state->ssl);
        mbedtls_ssl_config_free(&state->config);
        mbedtls_ctr_drbg_free(&state->ctr_drbg);
        if (state->cookie_initialized) {
            mbedtls_ssl_cookie_free(&state->cookie);
        }
        mbedtls_x509_crt_free(&state->local_cert);
        mbedtls_pk_free(&state->local_key);
    }
    mbedtls_free(state->client_hello_fragment_data);
    memset(state, 0, sizeof(*state));
}

int intercom_dtls_push_input(void *storage, const unsigned char *data, size_t length)
{
    if (storage == NULL || data == NULL || length == 0 || length > INTERCOM_DTLS_DATAGRAM_MAX) {
        return MBEDTLS_ERR_SSL_BAD_INPUT_DATA;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    if (state->input_pending) {
        return MBEDTLS_ERR_SSL_WANT_READ;
    }
    const int reassembly = intercom_reassemble_client_hello(state, data, length);
    if (reassembly < 0) {
        return reassembly;
    }
    if (reassembly > 0) {
        return 0;
    }
    memcpy(state->input, data, length);
    state->input_length = length;
    state->input_pending = true;
    return 0;
}

int intercom_dtls_set_peer_transport_id(void *storage, const unsigned char *data,
                                        size_t length)
{
    if (storage == NULL || data == NULL || length == 0 || length > 32) {
        return MBEDTLS_ERR_SSL_BAD_INPUT_DATA;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    if (!state->initialized || !state->is_server || state->handshake_complete) {
        return MBEDTLS_ERR_SSL_BAD_INPUT_DATA;
    }
    memcpy(state->peer_transport_id, data, length);
    state->peer_transport_id_length = length;
    return mbedtls_ssl_set_client_transport_id(&state->ssl, data, length);
}

int intercom_dtls_step(void *storage, uint64_t now_ms)
{
    if (storage == NULL) {
        return MBEDTLS_ERR_SSL_BAD_INPUT_DATA;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    state->now_ms = now_ms;
    if (state->handshake_complete) {
        return 0;
    }
    const int result = mbedtls_ssl_handshake(&state->ssl);
    if (result == MBEDTLS_ERR_SSL_HELLO_VERIFY_REQUIRED) {
        const int reset_result = mbedtls_ssl_session_reset(&state->ssl);
        if (reset_result != 0) {
            return reset_result;
        }
        mbedtls_ssl_set_bio(&state->ssl, state, intercom_bio_send, intercom_bio_recv, NULL);
        mbedtls_ssl_set_timer_cb(&state->ssl, state, intercom_timer_set, intercom_timer_get);
        mbedtls_ssl_set_export_keys_cb(&state->ssl, intercom_export_srtp_keys, state);
        mbedtls_ssl_set_mtu(&state->ssl, 1200);
        if (state->peer_transport_id_length != 0) {
            const int id_result = mbedtls_ssl_set_client_transport_id(
                &state->ssl, state->peer_transport_id, state->peer_transport_id_length);
            if (id_result != 0) {
                return id_result;
            }
        }
        return 1;
    }
    if (result == 0) {
        state->handshake_complete = true;
        if (!state->keys_ready) {
            return state->export_error != 0 ? state->export_error : MBEDTLS_ERR_SSL_BAD_INPUT_DATA;
        }
        mbedtls_dtls_srtp_info srtp_info;
        mbedtls_ssl_get_dtls_srtp_negotiation_result(&state->ssl, &srtp_info);
        if (srtp_info.MBEDTLS_PRIVATE(chosen_dtls_srtp_profile) !=
            MBEDTLS_TLS_SRTP_AES128_CM_HMAC_SHA1_80) {
            return MBEDTLS_ERR_SSL_BAD_INPUT_DATA;
        }
        const mbedtls_x509_crt *peer_cert = mbedtls_ssl_get_peer_cert(&state->ssl);
        if (peer_cert != NULL && peer_cert->raw.p != NULL) {
            if (mbedtls_sha256(peer_cert->raw.p, peer_cert->raw.len,
                               state->peer_fingerprint, 0) != 0) {
                return MBEDTLS_ERR_SSL_BAD_INPUT_DATA;
            }
            state->peer_fingerprint_ready = true;
        }
        return 0;
    }
    if (result == MBEDTLS_ERR_SSL_WANT_READ || result == MBEDTLS_ERR_SSL_WANT_WRITE) {
        return 1;
    }
    return result;
}

size_t intercom_dtls_pop_output(void *storage, unsigned char *output, size_t capacity)
{
    if (storage == NULL || output == NULL) {
        return 0;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    if (!state->output_pending || state->output_length > capacity) {
        return 0;
    }
    memcpy(output, state->output, state->output_length);
    const size_t length = state->output_length;
    state->output_pending = false;
    state->output_length = 0;
    return length;
}

int intercom_dtls_exported_keys(void *storage, unsigned char output[60])
{
    if (storage == NULL || output == NULL) {
        return 0;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    if (!state->keys_ready) {
        return 0;
    }
    memcpy(output, state->exported_keys, sizeof(state->exported_keys));
    return 1;
}

void intercom_dtls_export_diagnostic(void *storage, int *called, int *type,
                                     int *prf, int *error)
{
    if (storage == NULL || called == NULL || type == NULL || prf == NULL || error == NULL) {
        return;
    }
    const intercom_dtls_state *state = (const intercom_dtls_state *) storage;
    *called = state->export_callback_called;
    *type = state->export_type;
    *prf = state->export_prf;
    *error = state->export_error;
}

size_t intercom_dtls_debug_diagnostic(void *storage, int *level, int *line,
                                      unsigned char *message, size_t capacity)
{
    if (storage == NULL || level == NULL || line == NULL || message == NULL || capacity == 0) {
        return 0;
    }
    const intercom_dtls_state *state = (const intercom_dtls_state *) storage;
    *level = state->debug_level;
    *line = state->debug_line;
    size_t length = strlen(state->debug_message);
    if (length >= capacity) {
        length = capacity - 1;
    }
    memcpy(message, state->debug_message, length);
    message[length] = '\0';
    return length;
}

int intercom_dtls_local_fingerprint(void *storage, unsigned char output[32])
{
    if (storage == NULL || output == NULL) {
        return 0;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    memcpy(output, state->local_fingerprint, sizeof(state->local_fingerprint));
    return 1;
}

int intercom_dtls_peer_fingerprint(void *storage, unsigned char output[32])
{
    if (storage == NULL || output == NULL) {
        return 0;
    }
    intercom_dtls_state *state = (intercom_dtls_state *) storage;
    if (!state->peer_fingerprint_ready) {
        return 0;
    }
    memcpy(output, state->peer_fingerprint, sizeof(state->peer_fingerprint));
    return 1;
}
