#pragma once

#include <stddef.h>
#include <stdint.h>

#define INTERCOM_DTLS_STATE_BYTES 15360
#define INTERCOM_DTLS_DATAGRAM_MAX 1500
#define INTERCOM_DTLS_OUTPUT_SLOTS 6
#define INTERCOM_DTLS_ARENA_BYTES 26624

int intercom_dtls_heap_init(unsigned char *buffer, size_t length);
void intercom_dtls_heap_free(void);
size_t intercom_dtls_state_size(void);
int intercom_dtls_generate_identity(const unsigned char seed[32],
                                    unsigned char *cert_der_out,
                                    size_t cert_der_capacity,
                                    size_t *cert_der_len_out,
                                    unsigned char *key_der_out,
                                    size_t key_der_capacity,
                                    size_t *key_der_len_out,
                                    unsigned char fingerprint_out[32]);
int intercom_dtls_init(void *storage, size_t storage_length, int role,
                       const unsigned char seed[32]);
int intercom_dtls_init_with_identity(void *storage, size_t storage_length, int role,
                                     const unsigned char seed[32],
                                     const unsigned char *cert_der,
                                     size_t cert_der_len,
                                     const unsigned char *key_der,
                                     size_t key_der_len,
                                     const unsigned char fingerprint[32]);
void intercom_dtls_free(void *storage);
int intercom_dtls_push_input(void *storage, const unsigned char *data, size_t length);
int intercom_dtls_set_peer_transport_id(void *storage, const unsigned char *data, size_t length);
void intercom_dtls_set_mtu(void *storage, uint16_t mtu);
int intercom_dtls_step(void *storage, uint64_t now_ms);
int intercom_dtls_last_result(void *storage);
size_t intercom_dtls_pending_output_length(void *storage);
uint32_t intercom_dtls_send_calls(void *storage);
uint32_t intercom_dtls_recv_calls(void *storage);
int intercom_dtls_debug_status(void *storage);
int intercom_dtls_rng_probe(void *storage);
size_t intercom_dtls_pop_output(void *storage, unsigned char *output, size_t capacity);
int intercom_dtls_exported_keys(void *storage, unsigned char output[60]);
void intercom_dtls_export_diagnostic(void *storage, int *called, int *type,
                                     int *prf, int *error);
size_t intercom_dtls_debug_diagnostic(void *storage, int *level, int *line,
                                      unsigned char *message, size_t capacity);
int intercom_dtls_local_fingerprint(void *storage, unsigned char output[32]);
int intercom_dtls_peer_fingerprint(void *storage, unsigned char output[32]);
