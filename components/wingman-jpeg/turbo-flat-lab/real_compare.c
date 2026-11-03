/* Offline benchmark on private, real HDMI JPEG captures. MIT, 2026.
 * Link stock/flat libjpeg-turbo archives with prefix_archive.py.
 * Fatal libjpeg errors terminate this standalone process, never the service. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include <time.h>
#include <jpeglib.h>
#include "dual_api.h"
#include "requant_core.h"
#include "flat_hook.h"

#ifndef REAL_OPT_HUFFMAN
#define REAL_OPT_HUFFMAN FALSE
#endif

#define EXTRA_API(X) \
    X(jpeg_CreateDecompress) X(jpeg_mem_src) X(jpeg_read_header) \
    X(jpeg_start_decompress) X(jpeg_read_scanlines) X(jpeg_finish_decompress) \
    X(jpeg_destroy_decompress) X(jpeg_read_coefficients) \
    X(jpeg_copy_critical_parameters) X(jpeg_write_coefficients)
#define DECLARE(name) extern __typeof__(name) stock_##name;
EXTRA_API(DECLARE)
#undef DECLARE

enum { MODES=5, MAX_ROUNDS=51 };
static unsigned width,height;
static int hsample[3],vsample[3];
static unsigned char *scratch;
static unsigned long capacity;
static unsigned changed_quantizers,clamped_quantizers;
static const char *names[MODES]={"rgb-stock","rgb-flat","coef-scalar","coef-neon","passthrough-copy"};

static void fail(const char *message) {fprintf(stderr,"%s\n",message);exit(2);}
static double now(void) {
    struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec+t.tv_nsec*1e-9;
}
static int order(const void *a,const void *b) {
    double x=*(const double*)a,y=*(const double*)b;return (x>y)-(x<y);
}
static void open_source(struct jpeg_decompress_struct *d,struct jpeg_error_mgr *e,
                        const unsigned char *input,unsigned long size) {
    memset(d,0,sizeof(*d));d->err=stock_jpeg_std_error(e);
    stock_jpeg_CreateDecompress(d,JPEG_LIB_VERSION,sizeof(*d));stock_jpeg_mem_src(d,input,size);
    if(stock_jpeg_read_header(d,TRUE)!=JPEG_HEADER_OK)fail("not a complete JPEG header");
    if(d->data_precision!=8||d->jpeg_color_space!=JCS_YCbCr||d->num_components!=3||d->progressive_mode||d->arith_code)
        fail("benchmark accepts 8-bit sequential YCbCr capture frames only");
    if(!d->image_width||!d->image_height||d->image_width>8192||d->image_height>8192||
       (uint64_t)d->image_width*d->image_height>33177600)fail("source exceeds pixel budget");
    for(int ci=0;ci<3;ci++)if(d->comp_info[ci].quant_tbl_no!=(ci?1:0))
        fail("benchmark requires component quantizer mapping 0,1,1");
}
static void decode(const unsigned char *input,unsigned long size,unsigned char *rgb) {
    struct jpeg_decompress_struct d;struct jpeg_error_mgr e;
    open_source(&d,&e,input,size);d.out_color_space=JCS_RGB;d.dct_method=JDCT_ISLOW;
    stock_jpeg_start_decompress(&d);
    if(d.output_width!=width||d.output_height!=height||d.output_components!=3)fail("decoded geometry mismatch");
    while(d.output_scanline<height){JSAMPROW row=rgb+(size_t)d.output_scanline*width*3;stock_jpeg_read_scanlines(&d,&row,1);}
    stock_jpeg_finish_decompress(&d);
    if(e.num_warnings)fail("capture failed clean libjpeg decode; discard this sample");
    stock_jpeg_destroy_decompress(&d);
}
static unsigned long encode(unsigned char *rgb,int mode,int quality,int sampling,
                            unsigned char *storage,int probe) {
    struct jpeg_compress_struct c={0};struct jpeg_error_mgr e;
    unsigned char *out=storage;unsigned long size=capacity;
    c.err=WK_CALL(jpeg_std_error)(&e);WK_CALL(jpeg_CreateCompress)(&c,JPEG_LIB_VERSION,sizeof(c));
    WK_CALL(jpeg_mem_dest)(&c,&out,&size);c.image_width=width;c.image_height=height;
    c.input_components=3;c.in_color_space=JCS_RGB;WK_CALL(jpeg_set_defaults)(&c);
    WK_CALL(jpeg_set_quality)(&c,quality,TRUE);c.dct_method=JDCT_ISLOW;c.optimize_coding=REAL_OPT_HUFFMAN;
    for(int i=0;i<3;i++) {
        c.comp_info[i].h_samp_factor=sampling?hsample[i]:1;
        c.comp_info[i].v_samp_factor=sampling?vsample[i]:1;
    }
    WK_CALL(jpeg_start_compress)(&c,TRUE);
    wk_flat_hook hook;
    if(probe>=0&&!wk_flat_attach(&c,&hook,probe))fail("probe hook attach failed");
    while(c.next_scanline<height){JSAMPROW row=rgb+(size_t)c.next_scanline*width*3;WK_CALL(jpeg_write_scanlines)(&c,&row,1);}
    if(probe>=0) {
        printf("pixel_probe adaptive=%d blocks=%llu flat_blocks=%llu bypassed_blocks=%llu flat_pct=%.3f\n",
            probe,(unsigned long long)hook.core.blocks,(unsigned long long)hook.core.flat_blocks,
            (unsigned long long)hook.core.bypassed_blocks,100.0*hook.core.flat_blocks/hook.core.blocks);
        wk_flat_detach(&c,&hook);
    }
    WK_CALL(jpeg_finish_compress)(&c);WK_CALL(jpeg_destroy_compress)(&c);
    if(out!=storage)fail("encode output capacity exceeded");
    return size;
}
static unsigned long coefficients(const unsigned char *input,unsigned long size,
                                  int vector,int quality,unsigned char *storage) {
    struct jpeg_decompress_struct d;struct jpeg_error_mgr de;
    struct jpeg_compress_struct c={0};struct jpeg_error_mgr ce;
    unsigned char *out=storage;unsigned long bytes=capacity;
    open_source(&d,&de,input,size);
    jvirt_barray_ptr *arrays=stock_jpeg_read_coefficients(&d);
    c.err=stock_jpeg_std_error(&ce);stock_jpeg_CreateCompress(&c,JPEG_LIB_VERSION,sizeof(c));
    stock_jpeg_mem_dest(&c,&out,&bytes);stock_jpeg_copy_critical_parameters(&d,&c);
    stock_jpeg_set_quality(&c,quality,TRUE);c.optimize_coding=REAL_OPT_HUFFMAN;
    unsigned oldq[2][64],newq[2][64];wk_requant_table tables[2];
    changed_quantizers=clamped_quantizers=0;
    for(int table=0;table<2;table++)for(int k=0;k<64;k++) {
        unsigned old=d.quant_tbl_ptrs[table]->quantval[k],next=c.quant_tbl_ptrs[table]->quantval[k];
        if(!old||old>255)fail("source quantizer outside baseline domain");
        if(next<old){next=old;clamped_quantizers++;}
        changed_quantizers+=next!=old;
        oldq[table][k]=old;newq[table][k]=next;c.quant_tbl_ptrs[table]->quantval[k]=(UINT16)next;
    }
    for(int table=0;table<2;table++)wk_requant_init(&tables[table],oldq[table],newq[table]);
    /* No arithmetic or coefficient writes when the effective tables are
     * unchanged. q=100 selects source tables under our coarsening-only rule. */
    if(changed_quantizers)
    for(int ci=0;ci<3;ci++) {
        jpeg_component_info *component=&d.comp_info[ci];int table=component->quant_tbl_no;
        for(JDIMENSION y=0;y<component->height_in_blocks;y++) {
            JBLOCKARRAY row=(*d.mem->access_virt_barray)((j_common_ptr)&d,arrays[ci],y,1,TRUE);
            for(JDIMENSION x=0;x<component->width_in_blocks;x++) {
                JCOEF *block=row[0][x];
                if(vector&&wk_zero_ac(block)) {
                    int value=block[0];unsigned mag=(unsigned)(value<0?-value:value);
                    unsigned q=(mag*oldq[table][0]+newq[table][0]/2)/newq[table][0];
                    block[0]=(JCOEF)(value<0?-(int)q:(int)q);continue;
                }
                if(vector){wk_requant_vector(block,&tables[table]);continue;}
                for(int k=0;k<64;k++) {
                    int value=block[k];if(!value)continue;
                    unsigned mag=(unsigned)(value<0?-value:value);
                    unsigned q=(mag*oldq[table][k]+newq[table][k]/2)/newq[table][k];
                    block[k]=(JCOEF)(value<0?-(int)q:(int)q);
                }
            }
        }
    }
    stock_jpeg_write_coefficients(&c,arrays);stock_jpeg_finish_compress(&c);
    stock_jpeg_destroy_compress(&c);stock_jpeg_finish_decompress(&d);
    if(de.num_warnings)fail("coefficient source decoder warned");
    stock_jpeg_destroy_decompress(&d);
    if(out!=storage)fail("coefficient output capacity exceeded");
    return bytes;
}
static double psnr(const unsigned char *a,const unsigned char *b) {
    double sum=0;
    for(size_t i=0;i<(size_t)width*height*3;i++){double delta=(double)a[i]-b[i];sum+=delta*delta;}
    return sum?10*log10(255.0*255*((size_t)width*height*3)/sum):INFINITY;
}
static void structure(const unsigned char *input,unsigned long size) {
    struct jpeg_decompress_struct d;struct jpeg_error_mgr e;
    open_source(&d,&e,input,size);jvirt_barray_ptr *arrays=stock_jpeg_read_coefficients(&d);
    uint64_t blocks=0,zero_ac=0;
    for(int ci=0;ci<3;ci++) {
        jpeg_component_info *component=&d.comp_info[ci];uint64_t count=0,zero=0;
        for(JDIMENSION y=0;y<component->height_in_blocks;y++) {
            JBLOCKARRAY row=(*d.mem->access_virt_barray)((j_common_ptr)&d,arrays[ci],y,1,FALSE);
            for(JDIMENSION x=0;x<component->width_in_blocks;x++){count++;zero+=wk_zero_ac(row[0][x]);}
        }
        blocks+=count;zero_ac+=zero;
        printf("component=%d blocks=%llu zero_ac=%llu zero_ac_pct=%.3f\n",ci,
            (unsigned long long)count,(unsigned long long)zero,100.0*zero/count);
    }
    printf("source width=%u height=%u bytes=%lu sampling=%dx%d,%dx%d,%dx%d zero_ac_pct=%.3f dc_quantizers=%u,%u\n",
        width,height,size,hsample[0],vsample[0],hsample[1],vsample[1],hsample[2],vsample[2],
        100.0*zero_ac/blocks,d.quant_tbl_ptrs[0]->quantval[0],d.quant_tbl_ptrs[1]->quantval[0]);
    stock_jpeg_finish_decompress(&d);if(e.num_warnings)fail("structure decode warned");
    stock_jpeg_destroy_decompress(&d);
}
int main(int argc,char **argv) {
    if(argc<3){fprintf(stderr,"usage: real-compare INPUT.jpg OUTPUT_PREFIX [quality=81] [rounds=21] [sampling=source|444]\n");return 1;}
    int quality=argc>3?atoi(argv[3]):81,rounds=argc>4?atoi(argv[4]):21;
    int sampling=argc>5?strcmp(argv[5],"444")!=0:1;
    if(quality<1||quality>100||rounds<1||rounds>MAX_ROUNDS)fail("invalid quality/rounds");
    FILE *file=fopen(argv[1],"rb");if(!file)fail("cannot open input");
    if(fseek(file,0,SEEK_END))fail("input seek failed");
    long length=ftell(file);
    if(length<1||length>32*1024*1024)fail("input exceeds byte budget");
    rewind(file);
    unsigned char *input=malloc((size_t)length);if(!input)fail("input allocation failed");
    if(fread(input,1,(size_t)length,file)!=(size_t)length)fail("input read failed");
    fclose(file);
    struct jpeg_decompress_struct d;struct jpeg_error_mgr e;open_source(&d,&e,input,(unsigned long)length);
    width=d.image_width;height=d.image_height;
    for(int i=0;i<3;i++){hsample[i]=d.comp_info[i].h_samp_factor;vsample[i]=d.comp_info[i].v_samp_factor;}
    stock_jpeg_destroy_decompress(&d);
    size_t rgb_size=(size_t)width*height*3;unsigned char *reference=malloc(rgb_size);scratch=malloc(rgb_size);
    capacity=(unsigned long)width*height*6+65536;
    unsigned char *outputs[MODES];unsigned long sizes[MODES]={0};double times[MODES][MAX_ROUNDS];
    if(!reference||!scratch)fail("RGB allocation failed");
    for(int mode=0;mode<MODES;mode++)if(!(outputs[mode]=malloc(capacity)))fail("output allocation failed");
    decode(input,(unsigned long)length,reference);structure(input,(unsigned long)length);
    for(int round=-2;round<rounds;round++)for(int step=0;step<MODES;step++) {
        int mode=(step+round+2)%MODES;double start=now();
        if(mode<2){decode(input,(unsigned long)length,scratch);sizes[mode]=encode(scratch,mode,quality,sampling,outputs[mode],-1);}
        else if(mode<4)sizes[mode]=coefficients(input,(unsigned long)length,mode==3,quality,outputs[mode]);
        else {memcpy(outputs[mode],input,(size_t)length);sizes[mode]=(unsigned long)length;}
        double elapsed=(now()-start)*1000;
        if(round>=0)times[mode][round]=elapsed;
        if(step==MODES-1) {
            if(sizes[0]!=sizes[1]||memcmp(outputs[0],outputs[1],sizes[0]))fail("stock/flat RGB JPEG bytes differ");
            if(sizes[2]!=sizes[3]||memcmp(outputs[2],outputs[3],sizes[2]))fail("scalar/NEON coefficient JPEG bytes differ");
        }
    }
    /* Pixel-only encoder timings isolate the source-patch benefit from decode. */
    double encode_times[2][MAX_ROUNDS];
    for(int round=-2;round<rounds;round++)for(int step=0;step<2;step++) {
        int mode=(step+round+2)%2;double start=now();
        unsigned long bytes=encode(reference,mode,quality,sampling,outputs[mode],-1);
        double elapsed=(now()-start)*1000;if(bytes!=sizes[mode])fail("encode-only size mismatch");
        if(round>=0)encode_times[mode][round]=elapsed;
    }
    for(int adaptive=0;adaptive<2;adaptive++)
        (void)encode(reference,0,quality,sampling,outputs[0],adaptive);
    unsigned p95=(unsigned)ceil(rounds*.95)-1;
    for(int mode=0;mode<MODES;mode++) {
        decode(outputs[mode],sizes[mode],scratch);qsort(times[mode],rounds,sizeof(double),order);
        double encode_median=-1;
        if(mode<2){qsort(encode_times[mode],rounds,sizeof(double),order);encode_median=encode_times[mode][rounds/2];}
        printf("result mode=%s q=%d rgb_sampling=%s rounds=%d median_ms=%.6f p95_ms=%.6f encode_only_ms=%.6f bytes=%lu psnr_db=%.6f mbps_24=%.6f changed_quantizers=%u clamped_quantizers=%u\n",
            names[mode],quality,sampling?"source":"444",rounds,times[mode][rounds/2],times[mode][p95],encode_median,sizes[mode],
            psnr(reference,scratch),sizes[mode]*8.0*24/1000000,changed_quantizers,clamped_quantizers);
        char path[4096];int n=snprintf(path,sizeof(path),"%s-%s.jpg",argv[2],names[mode]);
        if(n<0||(size_t)n>=sizeof(path))fail("output prefix too long");
        FILE *out=fopen(path,"wbx");if(!out)fail("output exists or cannot be created");
        if(fwrite(outputs[mode],1,sizes[mode],out)!=sizes[mode])fail("output write failed");
        fclose(out);
    }
    puts("byte_exact_pairs=rgb-stock:rgb-flat,coef-scalar:coef-neon; PSNR reference=decoded source JPEG; Mbps excludes transport headers");
    free(input);free(reference);free(scratch);for(int i=0;i<MODES;i++)free(outputs[i]);return 0;
}
