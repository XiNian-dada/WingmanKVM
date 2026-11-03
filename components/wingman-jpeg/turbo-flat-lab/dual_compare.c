/* Offline reference only: use the system libjpeg-turbo accurate integer DCT,
 * 4:4:4, quality 81, standard Huffman tables. This is not linked into our crate.
 * Compile against headers matching the installed library's v6/v8 ABI. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <time.h>
#include <jpeglib.h>
#include "dual_api.h"

static double now(void) {
    struct timespec ts; clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec + ts.tv_nsec * 1e-9;
}
static int compare(const void *a,const void *b) {
    double x=*(const double*)a,y=*(const double*)b; return (x>y)-(x<y);
}
int main(int argc,char **argv) {
    const char *pattern=argc>1?argv[1]:"desktop";
    const unsigned w=1920,h=1080;
    unsigned char *rgb=malloc((size_t)w*h*3);
    if (!rgb) return 2;
    uint32_t seed=0xa135ce42;
    for(unsigned y=0;y<h;y++)for(unsigned x=0;x<w;x++) {
        unsigned char *p=rgb+((size_t)y*w+x)*3;
        if(!strcmp(pattern,"flat")){p[0]=18;p[1]=22;p[2]=28;}
        else if(!strcmp(pattern,"desktop")||!strcmp(pattern,"sparse")) {
            int glyph=x%11<7&&y%22<14&&(x/11+y/22)%3!=0
                &&(strcmp(pattern,"sparse")|| (x<900&&y<300));
            p[0]=glyph?210:18;p[1]=glyph?80+y/8%170:22;p[2]=glyph?100:28;
        } else if(!strcmp(pattern,"gradient")){p[0]=x*255/w;p[1]=y*255/h;p[2]=(x+y)%256;}
        else if(!strcmp(pattern,"noise")){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;p[0]=seed;p[1]=seed>>8;p[2]=seed>>16;}
        else {free(rgb);return 2;}
    }
    uint64_t hash=UINT64_C(14695981039346656037);
    for(size_t i=0;i<(size_t)w*h*3;i++){hash^=rgb[i];hash*=UINT64_C(1099511628211);}
    const unsigned long capacity=(unsigned long)w*h*6+65536;
    unsigned char *storage[2]={malloc(capacity),malloc(capacity)};
    if(!storage[0]||!storage[1])return 2;
    double times[2][21];unsigned long sizes[2]={0};
    for(int round=-2;round<21;round++)for(int step=0;step<2;step++) {
        int mode=(step+(round+2))%2;
        struct jpeg_compress_struct c={0};struct jpeg_error_mgr e;
        unsigned char *out=storage[mode];unsigned long size=capacity;
        double start=now();
        c.err=WK_CALL(jpeg_std_error)(&e);WK_CALL(jpeg_CreateCompress)(&c,JPEG_LIB_VERSION,sizeof(c));
        WK_CALL(jpeg_mem_dest)(&c,&out,&size);c.image_width=w;c.image_height=h;
        c.input_components=3;c.in_color_space=JCS_RGB;WK_CALL(jpeg_set_defaults)(&c);
        WK_CALL(jpeg_set_quality)(&c,81,TRUE);
        for(int i=0;i<c.num_components;i++)c.comp_info[i].h_samp_factor=c.comp_info[i].v_samp_factor=1;
        c.optimize_coding=FALSE;c.dct_method=JDCT_ISLOW;
        WK_CALL(jpeg_start_compress)(&c,TRUE);
        while(c.next_scanline<h){JSAMPROW row=rgb+(size_t)c.next_scanline*w*3;WK_CALL(jpeg_write_scanlines)(&c,&row,1);}
        WK_CALL(jpeg_finish_compress)(&c);WK_CALL(jpeg_destroy_compress)(&c);
        double elapsed=(now()-start)*1000;
        if(out!=storage[mode]){fprintf(stderr,"capacity exceeded\n");return 3;}
        sizes[mode]=size;if(round>=0)times[mode][round]=elapsed;
        if(step==1&&(sizes[0]!=sizes[1]||memcmp(storage[0],storage[1],sizes[0]))) {
            fprintf(stderr,"JPEG mismatch pattern=%s round=%d\n",pattern,round);return 4;
        }
    }
    for(int mode=0;mode<2;mode++) {
        qsort(times[mode],21,sizeof(double),compare);
        printf("pattern=%s mode=%s input_fnv64=%016llx median_ms=%.3f p95_ms=%.3f bytes=%lu byte_exact=true\n",
            pattern,mode?"flat-adaptive":"stock",(unsigned long long)hash,times[mode][10],times[mode][19],sizes[mode]);
    }
    free(storage[0]);free(storage[1]);free(rgb);return 0;
}
