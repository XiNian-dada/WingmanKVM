/* Offline reference only: use the system libjpeg-turbo accurate integer DCT,
 * 4:4:4, quality 81, standard Huffman tables. This is not linked into our crate.
 * Compile against headers matching the installed library's v6/v8 ABI. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <time.h>
#include <jpeglib.h>

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
    unsigned char *storage=malloc(capacity);
    if(!storage){free(rgb);return 2;}
    double times[15];unsigned long output_size=0;
    for(int n=-1;n<15;n++) {
        struct jpeg_compress_struct c={0};struct jpeg_error_mgr e;
        unsigned char *out=storage;unsigned long size=capacity;
        double start=now();
        c.err=jpeg_std_error(&e);jpeg_create_compress(&c);
        jpeg_mem_dest(&c,&out,&size);c.image_width=w;c.image_height=h;
        c.input_components=3;c.in_color_space=JCS_RGB;jpeg_set_defaults(&c);
        jpeg_set_quality(&c,81,TRUE);
        for(int i=0;i<c.num_components;i++){c.comp_info[i].h_samp_factor=1;c.comp_info[i].v_samp_factor=1;}
        c.optimize_coding=FALSE;c.dct_method=JDCT_ISLOW;
        jpeg_start_compress(&c,TRUE);
        while(c.next_scanline<h){JSAMPROW row=rgb+(size_t)c.next_scanline*w*3;jpeg_write_scanlines(&c,&row,1);}
        jpeg_finish_compress(&c);output_size=size;jpeg_destroy_compress(&c);
        double elapsed=(now()-start)*1000;
        if(out!=storage){fprintf(stderr,"reference output exceeded reserved capacity\n");return 3;}
        if(n>=0)times[n]=elapsed;
        if(n==0&&argc>2){FILE *file=fopen(argv[2],"wb");if(!file)return 4;
            size_t written=fwrite(out,1,size,file);fclose(file);if(written!=size)return 4;}

    }
    qsort(times,15,sizeof(double),compare);
    printf("pattern=%s jpeg_abi=%d input_fnv64=%016llx median_ms=%.3f p95_ms=%.3f bytes=%lu\n",
        pattern,JPEG_LIB_VERSION,(unsigned long long)hash,times[7],times[14],output_size);
    free(storage);free(rgb);return 0;
}
