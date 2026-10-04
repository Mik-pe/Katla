/* Preserve integer and floating TIFF channels before role-specific GPU conversion. */
#include <math.h>

static int katla_tiff_precision(const unsigned char *data, int length) {
    katla_tiff_stream stream={data,(size_t)length,0,0};
    TIFF *tiff=katla_tiff_open(&stream,KATLA_TIFF_META_BUDGET);
    if (!tiff) return stream.limit ? -2 : 0;
    uint16_t bits=8,format=SAMPLEFORMAT_UINT;
    TIFFGetFieldDefaulted(tiff,TIFFTAG_BITSPERSAMPLE,&bits);
    TIFFGetFieldDefaulted(tiff,TIFFTAG_SAMPLEFORMAT,&format);
    TIFFClose(tiff);
    if (bits<=8 && format==SAMPLEFORMAT_UINT) return 1;
    if (bits==16 && format==SAMPLEFORMAT_UINT) return 2;
    if (bits==32 && format==SAMPLEFORMAT_IEEEFP) return 4;
    return -1;
}

static float katla_tiff_sample(const unsigned char *bytes, int channel_bytes) {
    if (channel_bytes==2) { uint16_t value;memcpy(&value,bytes,2);return (float)value/65535.f; }
    float value;memcpy(&value,bytes,4);return value;
}

static void katla_tiff_store(unsigned char *destination, int channel_bytes, float value) {
    if (channel_bytes==2) {
        uint16_t normalized=(uint16_t)lroundf(fminf(1.f,fmaxf(0.f,value))*65535.f);
        memcpy(destination,&normalized,2);
    } else { memcpy(destination,&value,4); }
}

static int katla_tiff_precise_pixel(unsigned char *output, const unsigned char *block,
        size_t block_bytes, size_t pixel, uint16_t samples, uint16_t planar,
        uint16_t photo, int channel_bytes, uint16_t extra_count, const uint16_t *extras,
        uint16_t *const palette[3], size_t destination) {
    float values[5]={0.f,0.f,0.f,1.f,1.f};
    for (uint16_t c=0;c<samples;c++) {
        size_t offset=planar==PLANARCONFIG_SEPARATE ? block_bytes*c+pixel*channel_bytes : (pixel*samples+c)*channel_bytes;
        values[c]=katla_tiff_sample(block+offset,channel_bytes);
    }
    float rgba[4]; int base;
    if (photo==PHOTOMETRIC_RGB) { rgba[0]=values[0];rgba[1]=values[1];rgba[2]=values[2];base=3; }
    else if (photo==PHOTOMETRIC_MINISBLACK || photo==PHOTOMETRIC_MINISWHITE) {
        float gray=photo==PHOTOMETRIC_MINISWHITE ? 1.f-values[0] : values[0];
        rgba[0]=gray;rgba[1]=gray;rgba[2]=gray;base=1;
    } else if (photo==PHOTOMETRIC_SEPARATED) {
        for (int c=0;c<3;c++) rgba[c]=(1.f-values[c])*(1.f-values[3]);base=4;
    } else if (photo==PHOTOMETRIC_PALETTE && channel_bytes==2) {
        size_t index=(size_t)lroundf(values[0]*65535.f);
        for (int c=0;c<3;c++) rgba[c]=(float)palette[c][index]/65535.f;base=1;
    } else return 0;
    rgba[3]=samples>base ? values[base] : 1.f;
    if (samples>base && extra_count && extras[0]==EXTRASAMPLE_ASSOCALPHA && rgba[3]!=0.f) {
        for (int c=0;c<3;c++) rgba[c]/=rgba[3];
    }
    for (int c=0;c<4;c++) katla_tiff_store(output+(destination+c)*channel_bytes,channel_bytes,rgba[c]);
    return 1;
}

static unsigned char *katla_tiff_decode_precise(const unsigned char *data,int length,
        int *width,int *height,int *channels,int channel_bytes) {
    katla_tiff_stream stream={data,(size_t)length,0,0};
    TIFF *tiff=katla_tiff_open(&stream,KATLA_TIFF_DECODE_BUDGET);if (!tiff) return NULL;
    uint32_t w,h;uint16_t orientation=1;
    if (katla_tiff_dimensions(tiff,&w,&h,&orientation)!=1 || (uint64_t)w*h*4*channel_bytes>KATLA_TIFF_DECODE_BUDGET) { TIFFClose(tiff);return NULL; }
    uint16_t bits=0,samples=0,planar=0,photo=0,format=0,extra_count=0,*extras=NULL,*palette[3]={NULL,NULL,NULL};
    TIFFGetFieldDefaulted(tiff,TIFFTAG_BITSPERSAMPLE,&bits);TIFFGetFieldDefaulted(tiff,TIFFTAG_SAMPLESPERPIXEL,&samples);
    TIFFGetFieldDefaulted(tiff,TIFFTAG_PLANARCONFIG,&planar);TIFFGetFieldDefaulted(tiff,TIFFTAG_PHOTOMETRIC,&photo);
    TIFFGetFieldDefaulted(tiff,TIFFTAG_SAMPLEFORMAT,&format);TIFFGetField(tiff,TIFFTAG_EXTRASAMPLES,&extra_count,&extras);
    int valid=(bits==16 && format==SAMPLEFORMAT_UINT && channel_bytes==2) || (bits==32 && format==SAMPLEFORMAT_IEEEFP && channel_bytes==4);
    int base=photo==PHOTOMETRIC_RGB ? 3 : photo==PHOTOMETRIC_SEPARATED ? 4 : 1;
    valid=valid && (planar==PLANARCONFIG_CONTIG || planar==PLANARCONFIG_SEPARATE) && samples>=base && samples<=base+1 && samples<=5;
    if (photo==PHOTOMETRIC_SEPARATED) { uint16_t inkset=INKSET_CMYK;TIFFGetFieldDefaulted(tiff,TIFFTAG_INKSET,&inkset);valid=valid && inkset==INKSET_CMYK; }
    if (photo==PHOTOMETRIC_PALETTE) valid=valid && channel_bytes==2 && TIFFGetField(tiff,TIFFTAG_COLORMAP,&palette[0],&palette[1],&palette[2]);
    if (photo!=PHOTOMETRIC_RGB && photo!=PHOTOMETRIC_MINISBLACK && photo!=PHOTOMETRIC_MINISWHITE && photo!=PHOTOMETRIC_SEPARATED && photo!=PHOTOMETRIC_PALETTE) valid=0;
    if (!valid) { TIFFClose(tiff);return NULL; }
    uint32_t block_w=w,block_h=1;
    uint64_t stride=TIFFScanlineSize64(tiff),block_bytes=stride;
    int tiled=TIFFIsTiled(tiff);
    if (!tiled) {
        uint32_t rows=h;TIFFGetFieldDefaulted(tiff,TIFFTAG_ROWSPERSTRIP,&rows);block_h=rows<h ? rows : h;
        valid=block_h>0;block_bytes=TIFFVStripSize64(tiff,block_h);
    }
    if (tiled) {
        valid=TIFFGetField(tiff,TIFFTAG_TILEWIDTH,&block_w) && TIFFGetField(tiff,TIFFTAG_TILELENGTH,&block_h) && block_w && block_h;
        stride=TIFFTileRowSize64(tiff);block_bytes=TIFFTileSize64(tiff);
    }
    uint64_t planes=planar==PLANARCONFIG_SEPARATE ? samples : 1;
    uint64_t row_bytes=(uint64_t)block_w*channel_bytes*(planar==PLANARCONFIG_CONTIG ? samples : 1);
    valid=valid && stride>=row_bytes && stride<=KATLA_TIFF_DECODE_BUDGET && block_bytes>=stride*block_h && block_bytes<=KATLA_TIFF_DECODE_BUDGET/planes;
    unsigned char *block=valid ? malloc((size_t)(block_bytes*planes)) : NULL;
    unsigned char *output=block ? malloc((size_t)w*h*4*channel_bytes) : NULL;
    int ok=output!=NULL;
    for (uint32_t y=0;ok && y<h;y+=block_h) for (uint32_t x=0;ok && x<w;x+=block_w) {
        for (uint16_t plane=0;ok && plane<planes;plane++) {
            unsigned char *destination=block+block_bytes*plane;
            if (tiled) { if (TIFFReadTile(tiff,destination,x,y,0,plane)<(tmsize_t)block_bytes) ok=0; }
            else {
                uint32_t rows=h-y<block_h ? h-y : block_h;
                uint64_t needed=TIFFVStripSize64(tiff,rows);
                if (TIFFReadEncodedStrip(tiff,TIFFComputeStrip(tiff,y,plane),destination,(tmsize_t)block_bytes)<(tmsize_t)needed) ok=0;
            }
        }
        for (uint32_t by=0;ok && by<block_h && y+by<h;by++) for (uint32_t bx=0;ok && bx<block_w && x+bx<w;bx++) {
            size_t sample=by*(stride/(channel_bytes*(planar==PLANARCONFIG_CONTIG ? samples : 1)))+bx;
            ok=katla_tiff_precise_pixel(output,block,(size_t)block_bytes,sample,samples,planar,photo,channel_bytes,extra_count,extras,palette,katla_tiff_pixel(x+bx,y+by,w,h,orientation));
        }
    }
    free(block);TIFFClose(tiff);
    if (!ok) { free(output);return NULL; }
    *width=(int)(orientation>=5?h:w);*height=(int)(orientation>=5?w:h);*channels=4;return output;
}
