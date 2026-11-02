#pragma once
#include <stddef.h>
#define OVERRIDE_OPUS_ALLOC 1
#define OVERRIDE_OPUS_FREE 1
#define OVERRIDE_OPUS_REALLOC 1
static inline void *opus_alloc(size_t size) { (void)size; return NULL; }
static inline void *opus_realloc(void *p, size_t size) { (void)p; (void)size; return NULL; }
static inline void opus_free(void *p) { (void)p; }
