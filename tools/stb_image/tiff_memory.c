/* TIFF input and allocation limits are local to one bounded byte stream. */
#include <tiffio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>

#define KATLA_IMAGE_MAX_DIM 8192u
#define KATLA_IMAGE_MAX_PIXELS (16u * 1024u * 1024u)
#define KATLA_TIFF_META_BUDGET (4u * 1024u * 1024u)
#define KATLA_TIFF_DECODE_BUDGET (64u * 1024u * 1024u)
typedef struct { const unsigned char *bytes; size_t length, offset; int limit; } katla_tiff_stream;
static int katla_tiff_signature(const unsigned char *data, int length) {
    return length >= 4 && ((data[0]=='I' && data[1]=='I' && (data[2]==42 || data[2]==43) && data[3]==0) || (data[0]=='M' && data[1]=='M' && data[2]==0 && (data[3]==42 || data[3]==43)));
}
static tmsize_t katla_tiff_read(thandle_t handle, void *dst, tmsize_t count) {
    katla_tiff_stream *stream=handle;
    if (count<0 || stream->offset>stream->length) return 0;
    size_t available=stream->length-stream->offset;
    size_t wanted=(size_t)count<available ? (size_t)count : available;
    memcpy(dst,stream->bytes+stream->offset,wanted); stream->offset+=wanted; return (tmsize_t)wanted;
}
static tmsize_t katla_tiff_write(thandle_t handle,void *data,tmsize_t count) { (void)handle;(void)data;(void)count;return 0; }
static toff_t katla_tiff_seek(thandle_t handle,toff_t offset,int origin) {
    katla_tiff_stream *stream=handle; uint64_t base=0;
    if (origin==SEEK_CUR) base=stream->offset; else if (origin==SEEK_END) base=stream->length; else if (origin!=SEEK_SET) return (toff_t)-1;
    uint64_t position;
    if (origin!=SEEK_SET && (int64_t)offset<0) { uint64_t backwards=(uint64_t)(-(int64_t)(offset+1))+1; if (backwards>base) return (toff_t)-1; position=base-backwards; }
    else { if (offset>UINT64_MAX-base) return (toff_t)-1; position=base+offset; }
    if (position>stream->length) return (toff_t)-1;
    stream->offset=(size_t)position;return (toff_t)position;
}
static int katla_tiff_close(thandle_t handle) { (void)handle;return 0; }
static toff_t katla_tiff_size(thandle_t handle) { return ((katla_tiff_stream*)handle)->length; }
static int katla_tiff_map(thandle_t handle,void **base,toff_t *size) { (void)handle;(void)base;(void)size;return 0; }
static void katla_tiff_unmap(thandle_t handle,void *base,toff_t size) { (void)handle;(void)base;(void)size; }
static int katla_tiff_error(TIFF *tiff,void *user,const char *module,const char *format,va_list args) {
    (void)tiff;(void)module;(void)args;
    katla_tiff_stream *stream=user; if (strstr(format,"limit defined in open options")) stream->limit=1; return 1;
}
static TIFF *katla_tiff_open(katla_tiff_stream *stream,size_t budget) {
    TIFFOpenOptions *options=TIFFOpenOptionsAlloc();if (!options) return NULL;
    TIFFOpenOptionsSetMaxSingleMemAlloc(options,(tmsize_t)budget);
    TIFFOpenOptionsSetMaxCumulatedMemAlloc(options,(tmsize_t)budget);
    TIFFOpenOptionsSetErrorHandlerExtR(options,katla_tiff_error,stream);
    TIFFOpenOptionsSetWarningHandlerExtR(options,katla_tiff_error,stream);
    TIFF *tiff=TIFFClientOpenExt("encoded asset","rm",stream,katla_tiff_read,katla_tiff_write,katla_tiff_seek,katla_tiff_close,katla_tiff_size,katla_tiff_map,katla_tiff_unmap,options);
    TIFFOpenOptionsFree(options);return tiff;
}
/* 1 admitted, 0 invalid stream, -2 explicit metadata/pixel budget. */
static int katla_tiff_dimensions(TIFF *tiff,uint32_t *width,uint32_t *height,uint16_t *orientation) {
    if (!TIFFGetField(tiff,TIFFTAG_IMAGEWIDTH,width) || !TIFFGetField(tiff,TIFFTAG_IMAGELENGTH,height) || !*width || !*height) return 0;
    if (*width>KATLA_IMAGE_MAX_DIM || *height>KATLA_IMAGE_MAX_DIM || (uint64_t)*width * *height>KATLA_IMAGE_MAX_PIXELS) return -2;
    TIFFGetFieldDefaulted(tiff,TIFFTAG_ORIENTATION,orientation);
    if (*orientation<1 || *orientation>8) return 0;return 1;
}
static int katla_tiff_info(const unsigned char *data,int length,int *width,int *height,int *channels) {
    katla_tiff_stream stream={data,(size_t)length,0,0};TIFF *tiff=katla_tiff_open(&stream,KATLA_TIFF_META_BUDGET);
    if (!tiff) return stream.limit ? -2 : 0;
    if (stream.limit) { TIFFClose(tiff);return -2; }
    uint32_t w,h;uint16_t orientation=1;int result=katla_tiff_dimensions(tiff,&w,&h,&orientation);TIFFClose(tiff);
    if (result!=1) return result;
    *width=(int)(orientation>=5 ? h : w);*height=(int)(orientation>=5 ? w : h);*channels=4;return 1;
}
static size_t katla_tiff_pixel(uint32_t x,uint32_t y,uint32_t w,uint32_t h,uint16_t orientation) {
    uint32_t dx=x,dy=y,out_w=w;
    switch (orientation) {
    case 2:dx=w-1-x;break;case 3:dx=w-1-x;dy=h-1-y;break;case 4:dy=h-1-y;break;
    case 5:dx=y;dy=x;out_w=h;break;case 6:dx=h-1-y;dy=x;out_w=h;break;
    case 7:dx=h-1-y;dy=w-1-x;out_w=h;break;case 8:dx=y;dy=w-1-x;out_w=h;break;
    default:break;
    }
    return ((size_t)dy*out_w+dx)*4;
}
static unsigned char *katla_tiff_decode(const unsigned char *data,int length,int *width,int *height,int *channels) {
    katla_tiff_stream stream={data,(size_t)length,0,0};TIFF *tiff=katla_tiff_open(&stream,KATLA_TIFF_DECODE_BUDGET);if (!tiff) return NULL;
    uint32_t w,h;uint16_t orientation=1;if (katla_tiff_dimensions(tiff,&w,&h,&orientation)!=1) { TIFFClose(tiff);return NULL; }
    size_t bytes=(size_t)w*h*4;unsigned char *output=malloc(bytes);if (!output) { TIFFClose(tiff);return NULL; }
    uint16_t bits=0,samples=0,planar=0,photo=0,extra_count=0,*extras=NULL;
    TIFFGetFieldDefaulted(tiff,TIFFTAG_BITSPERSAMPLE,&bits);TIFFGetFieldDefaulted(tiff,TIFFTAG_SAMPLESPERPIXEL,&samples);
    TIFFGetFieldDefaulted(tiff,TIFFTAG_PLANARCONFIG,&planar);TIFFGetFieldDefaulted(tiff,TIFFTAG_PHOTOMETRIC,&photo);
    TIFFGetField(tiff,TIFFTAG_EXTRASAMPLES,&extra_count,&extras);
    int raw_rgb=bits==8 && planar==PLANARCONFIG_CONTIG && photo==PHOTOMETRIC_RGB && (samples==3 || samples==4) && !TIFFIsTiled(tiff);
    int ok=1;
    if (raw_rgb) {
        uint64_t stride=TIFFScanlineSize64(tiff);
        if (stride<(uint64_t)w*samples || stride>KATLA_TIFF_DECODE_BUDGET) ok=0;
        unsigned char *row=ok ? malloc((size_t)stride) : NULL;if (!row) ok=0;
        for (uint32_t y=0;ok && y<h;y++) {
            if (TIFFReadScanline(tiff,row,y,0)<0) { ok=0;break; }
            for (uint32_t x=0;x<w;x++) {
                const unsigned char *pixel=row+(size_t)x*samples;unsigned char *dst=output+katla_tiff_pixel(x,y,w,h,orientation);
                unsigned int alpha=(samples==4 && extra_count && extras[0]!=EXTRASAMPLE_UNSPECIFIED) ? pixel[3] : 255;
                for (int channel=0;channel<3;channel++) { unsigned int color=pixel[channel];if (alpha && extra_count && extras[0]==EXTRASAMPLE_ASSOCALPHA) color=(color*255+alpha/2)/alpha;dst[channel]=(unsigned char)(color>255?255:color); }
                dst[3]=(unsigned char)alpha;
            }
        }
        free(row);
    } else {
        uint32_t *raster=malloc(bytes);if (!raster) ok=0;
        TIFFSetField(tiff,TIFFTAG_ORIENTATION,ORIENTATION_TOPLEFT);
        if (ok && !TIFFReadRGBAImageOriented(tiff,w,h,raster,ORIENTATION_TOPLEFT,1)) ok=0;
        for (uint32_t y=0;ok && y<h;y++) for (uint32_t x=0;x<w;x++) {
            uint32_t value=raster[(size_t)y*w+x];unsigned char *dst=output+katla_tiff_pixel(x,y,w,h,orientation);unsigned int alpha=TIFFGetA(value);
            unsigned int colors[3]={TIFFGetR(value),TIFFGetG(value),TIFFGetB(value)};
            for (int channel=0;channel<3;channel++) { unsigned int color=colors[channel];if (alpha && alpha<255) color=(color*255+alpha/2)/alpha;dst[channel]=(unsigned char)(color>255?255:color); }dst[3]=(unsigned char)alpha;
        }
        free(raster);
    }
    TIFFClose(tiff);if (!ok) { free(output);return NULL; }
    *width=(int)(orientation>=5?h:w);*height=(int)(orientation>=5?w:h);*channels=4;return output;
}
