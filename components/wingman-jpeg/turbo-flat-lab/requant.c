/* Offline coefficient-domain experiment, MIT, WingmanKVM contributors, 2026.
 * Only generated 8-bit sequential YCbCr, standard tables, 4:2:2 input is tested.
 * Not a production parser, public library, or lossless quality reduction. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <time.h>
#include <jpeglib.h>
#include "requant_core.h"

enum { WIDTH=1920, HEIGHT=1080, ROUNDS=15, MODES=5 };
static const unsigned long capacity=(unsigned long)WIDTH*HEIGHT*6+65536;
static unsigned char *scratch;
static double now(void) {
    struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec+t.tv_nsec*1e-9;
}
static int order(const void *a,const void *b) {
    double x=*(const double *)a,y=*(const double *)b;return (x>y)-(x<y);
}
static unsigned long encode_rgb(unsigned char *rgb,int quality,unsigned char *storage) {
    struct jpeg_compress_struct c={0};struct jpeg_error_mgr e;
    unsigned char *out=storage;unsigned long size=capacity;
    c.err=jpeg_std_error(&e);jpeg_create_compress(&c);jpeg_mem_dest(&c,&out,&size);
    c.image_width=WIDTH;c.image_height=HEIGHT;c.input_components=3;c.in_color_space=JCS_RGB;
    jpeg_set_defaults(&c);jpeg_set_quality(&c,quality,TRUE);c.dct_method=JDCT_ISLOW;
    c.comp_info[0].h_samp_factor=2;c.comp_info[0].v_samp_factor=1;
    c.optimize_coding=FALSE;jpeg_start_compress(&c,TRUE);
    while(c.next_scanline<HEIGHT){JSAMPROW row=rgb+(size_t)c.next_scanline*WIDTH*3;jpeg_write_scanlines(&c,&row,1);}
    jpeg_finish_compress(&c);jpeg_destroy_compress(&c);
    if(out!=storage){fprintf(stderr,"output capacity exceeded\n");exit(2);}return size;
}
static void decode_rgb(const unsigned char *input,unsigned long size,unsigned char *rgb) {
    struct jpeg_decompress_struct d={0};struct jpeg_error_mgr e;
    d.err=jpeg_std_error(&e);jpeg_create_decompress(&d);jpeg_mem_src(&d,input,size);
    jpeg_read_header(&d,TRUE);d.out_color_space=JCS_RGB;d.dct_method=JDCT_ISLOW;
    jpeg_start_decompress(&d);
    if(d.output_width!=WIDTH||d.output_height!=HEIGHT||d.output_components!=3)exit(3);
    while(d.output_scanline<HEIGHT){JSAMPROW row=rgb+(size_t)d.output_scanline*WIDTH*3;jpeg_read_scanlines(&d,&row,1);}
    jpeg_finish_decompress(&d);jpeg_destroy_decompress(&d);
}
static uint32_t divide_exact(uint32_t n,unsigned divisor,uint64_t reciprocal) {
    if(divisor==1)return n;
    uint32_t q=(uint32_t)(((uint64_t)n*reciprocal)>>32);
    return q-((uint64_t)q*divisor>n);
}
static unsigned long transcode(const unsigned char *input,unsigned long size,
                               int mode,unsigned char *storage) {
    if(mode==0){decode_rgb(input,size,scratch);return encode_rgb(scratch,81,storage);}
    struct jpeg_decompress_struct d={0};struct jpeg_error_mgr de;
    struct jpeg_compress_struct c={0};struct jpeg_error_mgr ce;
    unsigned char *out=storage;unsigned long bytes=capacity;
    d.err=jpeg_std_error(&de);jpeg_create_decompress(&d);jpeg_mem_src(&d,input,size);
    jpeg_read_header(&d,TRUE);
    if(d.data_precision!=8||d.jpeg_color_space!=JCS_YCbCr||d.num_components!=3||d.progressive_mode||d.arith_code)exit(4);
    jvirt_barray_ptr *arrays=jpeg_read_coefficients(&d);
    c.err=jpeg_std_error(&ce);jpeg_create_compress(&c);jpeg_mem_dest(&c,&out,&bytes);
    jpeg_copy_critical_parameters(&d,&c);jpeg_set_quality(&c,81,TRUE);
    c.optimize_coding=FALSE;
    unsigned oldq[2][64],newq[2][64];uint64_t recip[2][64];
    for(int ci=0;ci<3;ci++)if(d.comp_info[ci].quant_tbl_no!=(ci?1:0))exit(5);
    for(int t=0;t<2;t++)for(int k=0;k<64;k++) {
        unsigned old=d.quant_tbl_ptrs[t]->quantval[k],next=c.quant_tbl_ptrs[t]->quantval[k];
        if(!old||old>255)exit(6);
        /* Only coarsen: never amplify coefficient magnitudes or pretend to
         * recover precision absent from a coarser source quantization table. */
        if(next<old)next=old;
        oldq[t][k]=old;newq[t][k]=next;recip[t][k]=(UINT64_C(0x100000000)+next-1)/next;
        c.quant_tbl_ptrs[t]->quantval[k]=(UINT16)next;
    }
    wk_requant_table vector_tables[2];
    for(int t=0;t<2;t++)wk_requant_init(&vector_tables[t],oldq[t],newq[t]);
    for(int ci=0;ci<3;ci++) {
        jpeg_component_info *component=&d.comp_info[ci];int table=component->quant_tbl_no;
        for(JDIMENSION y=0;y<component->height_in_blocks;y++) {
            JBLOCKARRAY row=(*d.mem->access_virt_barray)((j_common_ptr)&d,arrays[ci],y,1,TRUE);
            for(JDIMENSION x=0;x<component->width_in_blocks;x++) {
                JCOEF *block=row[0][x];
                if(mode>=3&&wk_zero_ac(block)) {
                    int value=block[0];uint32_t mag=(uint32_t)(value<0?-value:value);
                    unsigned q=(mag*oldq[table][0]+newq[table][0]/2)/newq[table][0];
                    block[0]=(JCOEF)(value<0?-(int)q:(int)q);continue;
                }
                if(mode==4){wk_requant_vector(block,&vector_tables[table]);continue;}
                for(int k=0;k<64;k++) {
                    int value=block[k];if(!value)continue;
                    uint32_t mag=(uint32_t)(value<0?-value:value);
                    uint32_t n=mag*oldq[table][k]+newq[table][k]/2;
                    uint32_t q=mode==2?divide_exact(n,newq[table][k],recip[table][k]):n/newq[table][k];
                    if(q>mag){fprintf(stderr,"coefficient magnitude grew\n");exit(7);}
                    block[k]=(JCOEF)(value<0?-(int)q:(int)q);
                }
            }
        }
    }
    jpeg_write_coefficients(&c,arrays);jpeg_finish_compress(&c);jpeg_destroy_compress(&c);
    jpeg_finish_decompress(&d);jpeg_destroy_decompress(&d);
    if(out!=storage)exit(8);
    return bytes;
}
static double psnr(const unsigned char *a,const unsigned char *b) {
    double sum=0;for(size_t i=0;i<(size_t)WIDTH*HEIGHT*3;i++){double delta=(double)a[i]-b[i];sum+=delta*delta;}
    return sum?10*log10(255.0*255*((size_t)WIDTH*HEIGHT*3)/sum):INFINITY;
}
static void verify_vector(void) {
    JCOEF values[64];unsigned oldq[64],newq[64];wk_requant_table table;
    for(unsigned d=1;d<=255;d++)for(unsigned choice=0;choice<3;choice++) {
        for(int k=0;k<64;k++){oldq[k]=choice==0?1:choice==1?(d+1)/2:d;newq[k]=d;}
        wk_requant_init(&table,oldq,newq);
        for(int base=-32768;base<=32704;base+=64) {
            for(int k=0;k<64;k++)values[k]=(JCOEF)(base+k);
            wk_requant_vector(values,&table);
            for(int k=0;k<64;k++) {
                int v=base+k;unsigned mag=(unsigned)(v<0?-v:v);
                int q=(int)((mag*oldq[k]+d/2)/d);if(v<0)q=-q;
                if(values[k]!=q){fprintf(stderr,"vector mismatch d=%u old=%u v=%d\n",d,oldq[k],v);exit(13);}
            }
        }
    }
    /* Exercise eight distinct divisor/reciprocal lanes and unaligned blocks. */
    uint32_t seed=1234567;JCOEF unaligned[65];
    for(int round=0;round<10000;round++) {
        for(int k=0;k<64;k++) {
            seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;
            newq[k]=1+seed%255;oldq[k]=1+(seed>>8)%newq[k];unaligned[k+1]=(JCOEF)(seed>>16);
            values[k]=unaligned[k+1];
        }
        wk_requant_init(&table,oldq,newq);wk_requant_vector(unaligned+1,&table);
        for(int k=0;k<64;k++) {
            int v=values[k];unsigned mag=(unsigned)(v<0?-v:v);
            int q=(int)((mag*oldq[k]+newq[k]/2)/newq[k]);if(v<0)q=-q;
            if(unaligned[k+1]!=q)exit(14);
        }
    }
    puts("NEON requant: full i16 domain x 255 divisors x 3 old-quantizer choices, mixed lanes and unaligned blocks passed");
}
static void verify_division(void) {
    /* Exhaust the actual numerator upper bound for 8-bit quantizers and i16
     * coefficients, for every possible baseline divisor, outside timing. */
    for(unsigned divisor=1;divisor<=255;divisor++) {
        uint64_t r=(UINT64_C(0x100000000)+divisor-1)/divisor;
        for(uint32_t n=0;n<=32768u*255+divisor/2;n++)
            if(divide_exact(n,divisor,r)!=n/divisor){fprintf(stderr,"division mismatch\n");exit(9);}
    }
    puts("reciprocal division exhaustive numerator domain: passed");
}
int main(int argc,char **argv) {
    if(argc>1&&!strcmp(argv[1],"test")){verify_vector();return 0;}
    if(argc>1&&!strcmp(argv[1],"test-divide")){verify_division();return 0;}
    const char *pattern=argc>1?argv[1]:"sparse";
    unsigned char *rgb=malloc((size_t)WIDTH*HEIGHT*3);
    scratch=malloc((size_t)WIDTH*HEIGHT*3);
    unsigned char *source=malloc(capacity),*outputs[MODES]={0};
    if(!rgb||!scratch||!source)return 10;
    for(int i=0;i<MODES;i++)if(!(outputs[i]=malloc(capacity)))return 10;
    uint32_t seed=0xa135ce42;
    for(unsigned y=0;y<HEIGHT;y++)for(unsigned x=0;x<WIDTH;x++) {
        unsigned char *p=rgb+((size_t)y*WIDTH+x)*3;
        if(!strcmp(pattern,"flat")){p[0]=18;p[1]=22;p[2]=28;}
        else if(!strcmp(pattern,"desktop")||!strcmp(pattern,"sparse")) {
            int glyph=x%11<7&&y%22<14&&(x/11+y/22)%3!=0&&
                (strcmp(pattern,"sparse")||(x<900&&y<300));
            p[0]=glyph?210:18;p[1]=glyph?80+y/8%170:22;p[2]=glyph?100:28;
        } else if(!strcmp(pattern,"gradient")){p[0]=x*255/WIDTH;p[1]=y*255/HEIGHT;p[2]=(x+y)%256;}
        else if(!strcmp(pattern,"noise")){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;p[0]=seed;p[1]=seed>>8;p[2]=seed>>16;}
        else return 11;
    }
    unsigned long source_bytes=encode_rgb(rgb,95,source),sizes[MODES]={0};
    decode_rgb(source,source_bytes,rgb);
    double times[MODES][ROUNDS];
    for(int round=-1;round<ROUNDS;round++)for(int step=0;step<MODES;step++) {
        int mode=(step+round+1)%MODES;double start=now();
        sizes[mode]=transcode(source,source_bytes,mode,outputs[mode]);
        if(round>=0)times[mode][round]=(now()-start)*1000;
        if(step==MODES-1)for(int m=2;m<MODES;m++)
            if(sizes[1]!=sizes[m]||memcmp(outputs[1],outputs[m],sizes[1]))return 12;
    }
    const char *names[]={"rgb-roundtrip","coefficient-divide","coefficient-reciprocal","coefficient-flat","coefficient-flat-neon"};
    for(int mode=0;mode<MODES;mode++) {
        decode_rgb(outputs[mode],sizes[mode],scratch);qsort(times[mode],ROUNDS,sizeof(double),order);
        printf("pattern=%s mode=%s median_ms=%.3f p95_ms=%.3f source_bytes=%lu output_bytes=%lu source_psnr_db=%.3f coefficient_modes_byte_exact=true\n",
            pattern,names[mode],times[mode][ROUNDS/2],times[mode][ROUNDS-1],source_bytes,sizes[mode],psnr(rgb,scratch));
    }
    free(rgb);free(scratch);free(source);for(int i=0;i<MODES;i++)free(outputs[i]);return 0;
}
