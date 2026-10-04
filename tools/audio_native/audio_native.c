/* Device/codec ABI only. Engine voices, mixing and ownership live in Odin. */
#if defined(__clang__)
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wunused-parameter"
#pragma clang diagnostic ignored "-Wtautological-compare"
#endif
#define STB_VORBIS_HEADER_ONLY
#include "stb_vorbis.c"
#define MINIAUDIO_IMPLEMENTATION
#define MA_NO_ENGINE
#define MA_NO_RESOURCE_MANAGER
#define MA_NO_NODE_GRAPH
#define MA_NO_GENERATION
#define MA_NO_ENCODING
#define MA_NO_VORBIS
#include "miniaudio.h"
#undef STB_VORBIS_HEADER_ONLY
#include "stb_vorbis.c"
#if defined(__clang__)
#pragma clang diagnostic pop
#endif
#include <stdatomic.h>
uint32_t ka_abi_version(void) { return 1; }

#define KA_CODEC_BUDGET (256u * 1024u * 1024u)
typedef struct ka_alloc { size_t used; } ka_alloc;
typedef union ka_header { size_t size; max_align_t alignment; } ka_header;
static void* ka_malloc(size_t size, void* user) {
    ka_alloc* a = user;
    if (size > KA_CODEC_BUDGET - a->used || size > SIZE_MAX - sizeof(ka_header)) return NULL;
    ka_header* p = malloc(sizeof(*p) + size);
    if (!p) return NULL;
    p->size = size; a->used += size; return p + 1;
}
static void ka_free(void* ptr, void* user) {
    if (!ptr) return;
    ka_alloc* a = user; ka_header* p = (ka_header*)ptr - 1;
    a->used -= p->size; free(p);
}
static void* ka_realloc(void* ptr, size_t size, void* user) {
    if (!ptr) return ka_malloc(size, user);
    if (!size) { ka_free(ptr, user); return NULL; }
    ka_alloc* a = user; ka_header* p = (ka_header*)ptr - 1;
    if (size > KA_CODEC_BUDGET - (a->used - p->size) || size > SIZE_MAX - sizeof(*p)) return NULL;
    size_t old = p->size; ka_header* n = realloc(p, sizeof(*p) + size);
    if (!n) return NULL;
    n->size = size; a->used = a->used - old + size; return n + 1;
}
typedef struct ka_metadata { uint32_t format, channels, sample_rate, reserved; uint64_t frames; } ka_metadata;
typedef struct ka_decoder { ma_decoder decoder; stb_vorbis* vorbis; void* vorbis_memory; ka_alloc alloc; ka_metadata info; } ka_decoder;
void ka_decoder_close(ka_decoder* d) {
    if (!d) return;
    if (d->vorbis) stb_vorbis_close(d->vorbis); else ma_decoder_uninit(&d->decoder);
    free(d->vorbis_memory); free(d);
}
int ka_decoder_open(const void* bytes, size_t length, uint32_t format, ka_decoder** output, ka_metadata* info) {
    *output = NULL;
    if (!bytes || !length || length > 64u*1024u*1024u || format > 3) return -1;
    ka_decoder* d = calloc(1, sizeof(*d)); if (!d) return -4;
    if (format == 1) {
        const int capacity = 16*1024*1024; d->vorbis_memory = malloc(capacity);
        if (!d->vorbis_memory) { free(d); return -4; }
        stb_vorbis_alloc alloc = {(char*)d->vorbis_memory, capacity}; int error = 0;
        d->vorbis = stb_vorbis_open_memory(bytes, (int)length, &error, &alloc);
        if (!d->vorbis) { free(d->vorbis_memory); free(d); return -2; }
        stb_vorbis_info vi = stb_vorbis_get_info(d->vorbis);
        d->info.channels = (uint32_t)vi.channels; d->info.sample_rate = vi.sample_rate;
        d->info.frames = stb_vorbis_stream_length_in_samples(d->vorbis);
    } else {
        ma_decoder_config config = ma_decoder_config_init(ma_format_f32, 0, 0);
        config.encodingFormat = format == 0 ? ma_encoding_format_wav : format == 2 ? ma_encoding_format_mp3 : ma_encoding_format_flac;
        config.allocationCallbacks = (ma_allocation_callbacks){&d->alloc,ka_malloc,ka_realloc,ka_free};
        if (ma_decoder_init_memory(bytes, length, &config, &d->decoder) != MA_SUCCESS) { free(d); return -2; }
        ma_format fmt; ma_decoder_get_data_format(&d->decoder, &fmt, &d->info.channels, &d->info.sample_rate, NULL, 0);
        if (ma_decoder_get_length_in_pcm_frames(&d->decoder, &d->info.frames) != MA_SUCCESS) { ka_decoder_close(d); return -2; }
    }
    d->info.format = format;
    if (!d->info.channels || d->info.channels > 16 || !d->info.sample_rate || d->info.sample_rate > 384000 || !d->info.frames || d->info.frames > UINT64_C(384000)*3600*24) { ka_decoder_close(d); return -3; }
    *info = d->info; *output = d; return 0;
}
int ka_decoder_read(ka_decoder* d, float* output, uint64_t frames, uint64_t* read) {
    *read = 0;
    if (!d || !output || frames > 65536) return -1;
    if (d->vorbis) { *read = (uint64_t)stb_vorbis_get_samples_float_interleaved(d->vorbis, (int)d->info.channels, output, (int)(frames*d->info.channels)); return 0; }
    ma_result result = ma_decoder_read_pcm_frames(&d->decoder, output, frames, read);
    return result == MA_SUCCESS || result == MA_AT_END ? 0 : -2;
}
int ka_decoder_seek(ka_decoder* d, uint64_t frame) {
    if (!d || frame > d->info.frames) return -1;
    if (d->vorbis) return frame <= UINT32_MAX && stb_vorbis_seek(d->vorbis, (unsigned)frame) ? 0 : -2;
    return ma_decoder_seek_to_pcm_frame(&d->decoder, frame) == MA_SUCCESS ? 0 : -2;
}

typedef void (*ka_render)(void* user, float* output, uint32_t frames);
typedef struct ka_device { ma_context context; ma_device device; ka_render render; void* user; atomic_uint notification, stopping; atomic_uint_fast64_t callbacks, frames, nonzero; } ka_device;
typedef struct ka_device_info { char name[256]; uint32_t sample_rate, channels, backend, is_default; uint64_t callbacks, frames, nonzero; } ka_device_info;
static void ka_data_callback(ma_device* device, void* output, const void* input, ma_uint32 frames) {
    (void)input; ka_device* d = device->pUserData;
    d->render(d->user, output, frames);
    uint64_t nonzero = 0; float* pcm = output;
    for (uint64_t i=0; i<(uint64_t)frames*2; ++i) if (pcm[i] != 0) ++nonzero;
    atomic_fetch_add(&d->callbacks, 1); atomic_fetch_add(&d->frames, frames); atomic_fetch_add(&d->nonzero, nonzero);
}
static void ka_notification(const ma_device_notification* event) {
    if ((event->type == ma_device_notification_type_stopped && !atomic_load(&((ka_device*)event->pDevice->pUserData)->stopping)) || event->type == ma_device_notification_type_rerouted || event->type == ma_device_notification_type_interruption_began || event->type == ma_device_notification_type_interruption_ended) {
        ka_device* d = event->pDevice->pUserData; atomic_store(&d->notification, 1);
    }
}
int ka_device_open(ka_render render, void* user, uint32_t sample_rate, int index, ka_device** output) {
    *output = NULL; if (!render || !sample_rate) return -1;
    ka_device* d = calloc(1,sizeof(*d)); if (!d) return -4;
    if (ma_context_init(NULL,0,NULL,&d->context) != MA_SUCCESS) { free(d); return -5; }
    ma_device_config config = ma_device_config_init(ma_device_type_playback);
    ma_device_info* infos = NULL; ma_uint32 count = 0;
    if (index >= 0) {
        if (ma_context_get_devices(&d->context,&infos,&count,NULL,NULL) != MA_SUCCESS || (unsigned)index >= count) { ma_context_uninit(&d->context); free(d); return -5; }
        config.playback.pDeviceID = &infos[index].id;
    }
    config.playback.format = ma_format_f32; config.playback.channels = 2; config.sampleRate = sample_rate;
    config.dataCallback = ka_data_callback; config.notificationCallback = ka_notification; config.pUserData = d;
    d->render = render; d->user = user;
    if (ma_device_init(&d->context,&config,&d->device) != MA_SUCCESS) { ma_context_uninit(&d->context); free(d); return -5; }
    *output = d; return 0;
}
int ka_device_start(ka_device* d) { return d && ma_device_start(&d->device) == MA_SUCCESS ? 0 : -6; }
int ka_device_stop(ka_device* d) { if (!d) return -6; atomic_store(&d->stopping,1); ma_result result = ma_device_stop(&d->device); atomic_store(&d->stopping,0); return result == MA_SUCCESS ? 0 : -6; }
void ka_device_close(ka_device* d) { if (d) { ma_device_uninit(&d->device); ma_context_uninit(&d->context); free(d); } }
int ka_device_changed(ka_device* d) { return d ? (int)atomic_exchange(&d->notification,0) : 0; }
void ka_device_snapshot(ka_device* d, ka_device_info* out) {
    memset(out,0,sizeof(*out)); if (!d) return;
    memcpy(out->name,d->device.playback.name,sizeof(out->name)-1);
    out->sample_rate = d->device.sampleRate; out->channels = d->device.playback.channels; out->backend = d->context.backend;
    out->callbacks = atomic_load(&d->callbacks); out->frames = atomic_load(&d->frames); out->nonzero = atomic_load(&d->nonzero);
}
int ka_devices(ka_device_info* output, uint32_t capacity, uint32_t* count) {
    ma_context context; if (ma_context_init(NULL,0,NULL,&context) != MA_SUCCESS) return -5;
    ma_device_info* infos; ma_uint32 total; ma_result result = ma_context_get_devices(&context,&infos,&total,NULL,NULL);
    if (result == MA_SUCCESS) {
        *count = total;
        for (uint32_t i=0;i<total && i<capacity;i++) { memset(&output[i],0,sizeof(output[i])); memcpy(output[i].name,infos[i].name,255); output[i].is_default = infos[i].isDefault; output[i].backend = context.backend; }
    }
    ma_context_uninit(&context); return result == MA_SUCCESS ? 0 : -5;
}
