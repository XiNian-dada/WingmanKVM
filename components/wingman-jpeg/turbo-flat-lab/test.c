#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <jpeglib.h>
#ifdef WK_SOURCE
#include "dual_api.h"
#define MAX_MODE 1
#define BASELINE "same-build stock libjpeg"
#else
#include "flat_hook.h"
#define WK_CALL(name) name
#define MAX_MODE 2
#define BASELINE "installed libjpeg"
#endif

static unsigned char outputs[3][262144];
static size_t encode(const unsigned char *pixels,unsigned w,unsigned h,unsigned channels,
                     int quality,int sampling,int progressive,int optimize,unsigned qdc,
                     int mode,unsigned char *storage) {
    struct jpeg_compress_struct c={0};struct jpeg_error_mgr e;
    unsigned char *out=storage;unsigned long size=sizeof(outputs[0]);
    c.err=WK_CALL(jpeg_std_error)(&e);WK_CALL(jpeg_CreateCompress)(&c,JPEG_LIB_VERSION,sizeof(c));WK_CALL(jpeg_mem_dest)(&c,&out,&size);
    c.image_width=w;c.image_height=h;c.input_components=channels;
    c.in_color_space=channels==1?JCS_GRAYSCALE:channels==3?JCS_RGB:JCS_CMYK;
    WK_CALL(jpeg_set_defaults)(&c);WK_CALL(jpeg_set_quality)(&c,quality,TRUE);
    if(channels==3&&sampling==0)for(int i=0;i<c.num_components;i++)
        c.comp_info[i].h_samp_factor=c.comp_info[i].v_samp_factor=1;
    if(channels==3&&sampling==1){c.comp_info[0].h_samp_factor=2;c.comp_info[0].v_samp_factor=1;}
    if(qdc)c.quant_tbl_ptrs[0]->quantval[0]=(UINT16)qdc;
    c.dct_method=JDCT_ISLOW;c.optimize_coding=optimize;c.restart_interval=3;
    if(progressive)WK_CALL(jpeg_simple_progression)(&c);
    WK_CALL(jpeg_start_compress)(&c,TRUE);
    #ifndef WK_SOURCE
    wk_flat_hook hook;
    if(mode&&!wk_flat_attach(&c,&hook,mode==2)){fprintf(stderr,"attach failed\n");exit(2);}
    #endif
    while(c.next_scanline<h){JSAMPROW row=(JSAMPROW)pixels+(size_t)c.next_scanline*w*channels;WK_CALL(jpeg_write_scanlines)(&c,&row,1);}
    #ifndef WK_SOURCE
    if(mode)wk_flat_detach(&c,&hook);
    #endif
    WK_CALL(jpeg_finish_compress)(&c);WK_CALL(jpeg_destroy_compress)(&c);
    if(out!=storage){fprintf(stderr,"test capacity exceeded\n");exit(3);}
    return size;
}

int main(void) {
    unsigned char pixels[65536];unsigned long cases=0;
    /* Every possible 8-bit sample and every baseline DC quantizer. This
     * compares actual system-library output, including SIMD rounding. */
    for(unsigned q=1;q<=255;q++)for(unsigned sample=0;sample<256;sample++) {
        memset(pixels,sample,64);
        size_t length=encode(pixels,8,8,1,81,0,0,0,q,0,outputs[0]);
        for(int mode=1;mode<=MAX_MODE;mode++) {
            size_t actual=encode(pixels,8,8,1,81,0,0,0,q,mode,outputs[mode]);
            if(length!=actual||memcmp(outputs[0],outputs[mode],length)) {
                fprintf(stderr,"DC mismatch q=%u sample=%u mode=%d\n",q,sample,mode);return 1;
            }
        }
        cases++;
    }
    printf("DC exhaustive: %lu cases, optimized modes byte-exact\n",cases);
    const unsigned dims[][2]={{1,1},{7,9},{8,8},{31,17},{127,65}};
    const int qualities[]={1,20,50,81,95,100};uint32_t seed=0xabcde123;
    for(unsigned d=0;d<5;d++)for(unsigned channels=1;channels<=4;channels++) {
        if(channels==2)continue;
        unsigned w=dims[d][0],h=dims[d][1];size_t bytes=(size_t)w*h*channels;
        for(unsigned pattern=0;pattern<3;pattern++) {
            for(size_t i=0;i<bytes;i++) {
                seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;
                pixels[i]=pattern==0?127:pattern==1?(i==bytes-1?128:127):(unsigned char)seed;
            }
            for(unsigned q=0;q<6;q++)for(int sampling=0;sampling<3;sampling++)
            for(int coding=0;coding<3;coding++) {
                size_t length=encode(pixels,w,h,channels,qualities[q],sampling,coding==2,coding==1,0,0,outputs[0]);
                for(int mode=1;mode<=MAX_MODE;mode++) {
                    size_t actual=encode(pixels,w,h,channels,qualities[q],sampling,coding==2,coding==1,0,mode,outputs[mode]);
                    if(length!=actual||memcmp(outputs[0],outputs[mode],length)) {
                        fprintf(stderr,"image mismatch d=%u ch=%u pattern=%u q=%d sampling=%d coding=%d mode=%d\n",d,channels,pattern,qualities[q],sampling,coding,mode);return 1;
                    }
                }
                cases++;
            }
        }
    }
    printf("All %lu cases passed against %s; no altered JPEG bytes\n",cases,BASELINE);
    return 0;
}
