#pragma once

#include <stddef.h>
#include <stdint.h>

#define INTERCOM_DTLS_STATE_BYTES 16384
#define INTERCOM_DTLS_DATAGRAM_MAX 1500
#define INTERCOM_DTLS_ARENA_BYTES 28672

int intercom_dtls_heap_init(unsigned char *buffer, size_t length);
void intercom_dtls_heap_free(void);
size_t intercom_dtls_state_size(void);
int intercom_dtls_init(void *storage, size_t storage_length, int role,
                       const unsigned char seed[32]);
void intercom_dtls_free(void *storage);
int intercom_dtls_push_input(void *storage, const unsigned char *data, size_t length);
int intercom_dtls_set_peer_transport_id(void *storage, const unsigned char *data, size_t length);
int intercom_dtls_step(void *storage, uint64_t now_ms);
int intercom_dtls_last_result(void *storage);
size_t intercom_dtls_pending_output_length(void *storage);
uint32_t intercom_dtls_send_calls(void *storage);
uint32_t intercom_dtls_recv_calls(void *storage);
int intercom_dtls_debug_status(void *storage);
int intercom_dtls_rng_probe(void *storage);
size_t intercom_dtls_pop_output(void *storage, unsigned char *output, size_t capacity);
int intercom_dtls_exported_keys(void *storage, unsigned char output[60]);
int intercom_dtls_local_fingerprint(void *storage, unsigned char output[32]);
int intercom_dtls_peer_fingerprint(void *storage, unsigned char output[32]);
