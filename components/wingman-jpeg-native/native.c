/* MIT, WingmanKVM contributors, 2026. libjpeg-turbo retains its own licenses.
 * All setjmp/longjmp frames and callbacks are C. No jump crosses Rust frames.
 * Mutable cleanup state lives on the heap, not in modified automatic locals. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <setjmp.h>
#include <time.h>
#include <jpeglib.h>
#include "requant_core.h"

#define WK_MAX_INPUT (32u * 1024u * 1024u)
#define WK_MAX_OUTPUT (64u * 1024u * 1024u)
#define WK_MAX_PIXELS 33177600u
const unsigned char wk_jpeg_flat_marker = 0;

struct wk_context;
typedef struct {
    struct jpeg_error_mgr pub;
    struct wk_context *owner;
} wk_error;
typedef struct {
    struct jpeg_destination_mgr pub;
    struct wk_context *owner;
} wk_dest;
typedef struct wk_context {
    struct jpeg_decompress_struct d;
    struct jpeg_compress_struct c;
    wk_error de,ce;
    struct jpeg_progress_mgr progress;
    wk_dest destination;
    jmp_buf jump;
    int d_created,c_created;
    unsigned char *pixels,*output;
    size_t capacity,length;
    char message[256];
    struct timespec started;
} wk_context;

static void wk_fail(wk_context *ctx,const char *message) {
    snprintf(ctx->message,sizeof(ctx->message),"%s",message);
    longjmp(ctx->jump,1);
}
static void wk_error_exit(j_common_ptr info) {
    wk_error *e=(wk_error *)info->err;
    (*info->err->format_message)(info,e->owner->message);
    longjmp(e->owner->jump,1);
}
static void wk_warning(j_common_ptr info,int level) {
    if(level<0)info->err->num_warnings++;
}
static void wk_progress(j_common_ptr info) {
    wk_context *ctx=((wk_error *)info->err)->owner;
    if(info->is_decompressor&&((j_decompress_ptr)info)->input_scan_number>64)
        wk_fail(ctx,"JPEG exceeds 64 scans");
    struct timespec current;clock_gettime(CLOCK_MONOTONIC,&current);
    if(current.tv_sec-ctx->started.tv_sec>3)wk_fail(ctx,"JPEG processing exceeded time budget");
}
static void wk_init_destination(j_compress_ptr info) {
    wk_context *ctx=((wk_dest *)info->dest)->owner;
    ctx->capacity=65536;ctx->output=malloc(ctx->capacity);
    if(!ctx->output)wk_fail(ctx,"JPEG output allocation failed");
    info->dest->next_output_byte=ctx->output;info->dest->free_in_buffer=ctx->capacity;
}
static boolean wk_grow_destination(j_compress_ptr info) {
    wk_context *ctx=((wk_dest *)info->dest)->owner;
    size_t old=ctx->capacity;
    if(old>=WK_MAX_OUTPUT)wk_fail(ctx,"JPEG output exceeds 64 MiB");
    size_t next=old*2;
    unsigned char *replacement=realloc(ctx->output,next);
    if(!replacement)wk_fail(ctx,"JPEG output allocation failed");
    ctx->output=replacement;ctx->capacity=next;
    info->dest->next_output_byte=replacement+old;info->dest->free_in_buffer=next-old;
    return TRUE;
}
static void wk_finish_destination(j_compress_ptr info) {
    wk_context *ctx=((wk_dest *)info->dest)->owner;
    ctx->length=ctx->capacity-info->dest->free_in_buffer;
}
static void wk_cleanup(wk_context *ctx) {
    if(ctx->c_created)jpeg_destroy_compress(&ctx->c);
    if(ctx->d_created)jpeg_destroy_decompress(&ctx->d);
    free(ctx->pixels);free(ctx->output);free(ctx);
}
static void wk_pixel_encode(wk_context *ctx,int quality,int gray,int flat,int optimize) {
    j_decompress_ptr d=&ctx->d;j_compress_ptr c=&ctx->c;
    d->out_color_space=gray?JCS_GRAYSCALE:JCS_RGB;d->dct_method=JDCT_ISLOW;
    jpeg_start_decompress(d);
    size_t stride=(size_t)d->output_width*d->output_components;
    size_t bytes=stride*d->output_height;
    ctx->pixels=malloc(bytes);
    if(!ctx->pixels)wk_fail(ctx,"JPEG pixel allocation failed");
    while(d->output_scanline<d->output_height) {
        JSAMPROW row=ctx->pixels+(size_t)d->output_scanline*stride;
        if(jpeg_read_scanlines(d,&row,1)!=1)wk_fail(ctx,"JPEG decoder suspended");
    }
    jpeg_finish_decompress(d);
    if(d->err->num_warnings)wk_fail(ctx,"Corrupt or truncated JPEG frame");
    c->image_width=d->output_width;c->image_height=d->output_height;
    c->input_components=gray?1:3;c->in_color_space=gray?JCS_GRAYSCALE:JCS_RGB;
    jpeg_set_defaults(c);jpeg_set_quality(c,quality,TRUE);
    for(int i=0;i<c->num_components;i++)c->comp_info[i].h_samp_factor=c->comp_info[i].v_samp_factor=1;
    c->dct_method=JDCT_ISLOW;c->optimize_coding=optimize;
    if(flat)c->client_data=(void *)&wk_jpeg_flat_marker;
    jpeg_start_compress(c,TRUE);
    while(c->next_scanline<c->image_height) {
        JSAMPROW row=ctx->pixels+(size_t)c->next_scanline*stride;
        if(jpeg_write_scanlines(c,&row,1)!=1)wk_fail(ctx,"JPEG encoder suspended");
    }
    jpeg_finish_compress(c);
}
/* Returns false before consuming coefficients for formats that require a
 * pixel fallback. It never silently switches to raw passthrough. */
static int wk_coefficient_encode(wk_context *ctx,int quality,int gray,int preserve,int optimize) {
    j_decompress_ptr d=&ctx->d;j_compress_ptr c=&ctx->c;
    if(d->jpeg_color_space!=JCS_YCbCr&&d->jpeg_color_space!=JCS_GRAYSCALE)return 0;
    if(gray&&(d->comp_info[0].h_samp_factor!=d->max_h_samp_factor||
              d->comp_info[0].v_samp_factor!=d->max_v_samp_factor))return 0;
    for(int ci=0;ci<d->num_components;ci++) {
        int t=d->comp_info[ci].quant_tbl_no;
        if(t<0||t>=NUM_QUANT_TBLS||!d->quant_tbl_ptrs[t])return 0;
        for(int k=0;k<64;k++)if(!d->quant_tbl_ptrs[t]->quantval[k]||d->quant_tbl_ptrs[t]->quantval[k]>255)return 0;
    }
    jvirt_barray_ptr *arrays=jpeg_read_coefficients(d);
    jpeg_copy_critical_parameters(d,c);
    if(gray)jpeg_set_colorspace(c,JCS_GRAYSCALE);
    jpeg_set_quality(c,quality,TRUE);c->optimize_coding=preserve?TRUE:optimize;
    unsigned target[2][64];
    for(int t=0;t<2;t++)for(int k=0;k<64;k++)
        target[t][k]=c->quant_tbl_ptrs[t]?c->quant_tbl_ptrs[t]->quantval[k]:1;
    wk_requant_table tables[3];int changed[3]={0};
    for(int ci=0;ci<c->num_components;ci++) {
        int source_table=d->comp_info[ci].quant_tbl_no;
        unsigned oldq[64],newq[64];
        JQUANT_TBL *table=jpeg_alloc_quant_table((j_common_ptr)c);
        for(int k=0;k<64;k++) {
            unsigned old=d->quant_tbl_ptrs[source_table]->quantval[k];
            unsigned next=preserve?old:target[ci?1:0][k];
            if(next<old)next=old;
            oldq[k]=old;newq[k]=next;table->quantval[k]=(UINT16)next;changed[ci]|=old!=next;
        }
        c->quant_tbl_ptrs[ci]=table;c->comp_info[ci].quant_tbl_no=ci;
        wk_requant_init(&tables[ci],oldq,newq);
    }
    for(int ci=0;ci<c->num_components;ci++) {
        if(!changed[ci])continue;
        jpeg_component_info *component=&d->comp_info[ci];
        for(JDIMENSION y=0;y<component->height_in_blocks;y++) {
            JBLOCKARRAY row=(*d->mem->access_virt_barray)((j_common_ptr)d,arrays[ci],y,1,TRUE);
            for(JDIMENSION x=0;x<component->width_in_blocks;x++) {
                JCOEF *block=row[0][x];
                if(wk_zero_ac(block)) {
                    int v=block[0];unsigned mag=(unsigned)(v<0?-v:v);
                    unsigned q=(mag*tables[ci].oldq[0]+tables[ci].newq[0]/2)/tables[ci].newq[0];
                    block[0]=(JCOEF)(v<0?-(int)q:(int)q);
                } else wk_requant_vector(block,&tables[ci]);
            }
        }
    }
    jpeg_write_coefficients(c,arrays);
    if(preserve) {
        for(jpeg_saved_marker_ptr marker=d->marker_list;marker;marker=marker->next) {
            /* Keep orientation and color interpretation in preserve mode.
             * A color ICC profile must not be attached to a grayscale image. */
            if(gray&&marker->marker==JPEG_APP0+2)continue;
            jpeg_write_marker(c,marker->marker,marker->data,marker->data_length);
        }
    }
    jpeg_finish_compress(c);jpeg_finish_decompress(d);
    if(d->err->num_warnings)wk_fail(ctx,"Corrupt or truncated JPEG frame");
    return 1;
}

/* mode: 0 standard, 1 exact-flat, 2 requantize, 3 source-table/Huffman.
 * Ownership of successful output transfers to caller; release with wk_jpeg_free. */
int wk_jpeg_process(const unsigned char *input,size_t input_len,int quality,int gray,int mode,int optimize,
                    unsigned char **output,size_t *output_len,char *error,size_t error_size) {
    *output=NULL;*output_len=0;
    if(!input||input_len<4||input_len>WK_MAX_INPUT||quality<1||quality>100||mode<0||mode>3) {
        snprintf(error,error_size,"Invalid JPEG input or options");return 1;
    }
    wk_context *ctx=calloc(1,sizeof(*ctx));
    if(!ctx){snprintf(error,error_size,"JPEG context allocation failed");return 1;}
    clock_gettime(CLOCK_MONOTONIC,&ctx->started);
    if(setjmp(ctx->jump)) {
        snprintf(error,error_size,"%s",ctx->message);wk_cleanup(ctx);return 1;
    }
    ctx->de.owner=ctx;ctx->ce.owner=ctx;
    ctx->d.err=jpeg_std_error(&ctx->de.pub);ctx->c.err=jpeg_std_error(&ctx->ce.pub);
    ctx->de.pub.error_exit=ctx->ce.pub.error_exit=wk_error_exit;
    ctx->de.pub.emit_message=ctx->ce.pub.emit_message=wk_warning;
    ctx->d_created=1;jpeg_create_decompress(&ctx->d);
    ctx->c_created=1;jpeg_create_compress(&ctx->c);
    ctx->d.mem->max_memory_to_use=256L*1024*1024;ctx->c.mem->max_memory_to_use=256L*1024*1024;
    ctx->progress.progress_monitor=wk_progress;ctx->d.progress=ctx->c.progress=&ctx->progress;
    ctx->destination.owner=ctx;ctx->destination.pub.init_destination=wk_init_destination;
    ctx->destination.pub.empty_output_buffer=wk_grow_destination;ctx->destination.pub.term_destination=wk_finish_destination;
    ctx->c.dest=&ctx->destination.pub;
    jpeg_mem_src(&ctx->d,input,(unsigned long)input_len);
    if(mode==3) {
        jpeg_save_markers(&ctx->d,JPEG_APP0+1,65535);
        jpeg_save_markers(&ctx->d,JPEG_APP0+2,65535);
    }
    if(jpeg_read_header(&ctx->d,TRUE)!=JPEG_HEADER_OK)wk_fail(ctx,"Incomplete JPEG header");
    size_t marker_bytes=0;unsigned marker_count=0;
    for(jpeg_saved_marker_ptr marker=ctx->d.marker_list;marker;marker=marker->next) {
        marker_bytes+=marker->data_length;
        if(++marker_count>128||marker_bytes>1024*1024)wk_fail(ctx,"JPEG metadata exceeds preserve limits");
    }
    if(ctx->d.data_precision!=8||!ctx->d.image_width||!ctx->d.image_height||
       ctx->d.image_width>8192||ctx->d.image_height>8192||
       (uint64_t)ctx->d.image_width*ctx->d.image_height>WK_MAX_PIXELS)
        wk_fail(ctx,"JPEG exceeds 8-bit / 8192-edge / 33 MP input limits");
    if(mode<2)wk_pixel_encode(ctx,quality,gray,mode!=0,optimize);
    else if(!wk_coefficient_encode(ctx,quality,gray,mode==3,optimize)) {
        if(mode==3)wk_fail(ctx,"Preserve mode requires an 8-bit YCbCr or grayscale JPEG with baseline quantizers");
        wk_pixel_encode(ctx,quality,gray,1,optimize);
    }
    *output=ctx->output;*output_len=ctx->length;ctx->output=NULL;
    wk_cleanup(ctx);return 0;
}
void wk_jpeg_free(unsigned char *buffer) {free(buffer);}
