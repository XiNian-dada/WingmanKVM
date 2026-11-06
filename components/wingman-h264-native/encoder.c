/* GPL-2.0-or-later. Standalone encoder; Rust communicates only over pipes.
 * x264 is pinned in vendor/. JPEG errors never cross a language boundary. */
#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#endif
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>
#include <time.h>
#include <jpeglib.h>
#include "x264.h"
#define MAX_JPEG (32u * 1024u * 1024u)
#define MAX_FRAME (4u * 1024u * 1024u)
#define MAX_PIXELS 33177600u
const unsigned char wk_jpeg_flat_marker = 0;
typedef struct {
    struct jpeg_decompress_struct jpeg;
    struct jpeg_error_mgr errors;
    jmp_buf jump;
    x264_t *encoder;
    x264_picture_t picture;
    uint8_t *input, *rows, *raw[3];
    int created, allocated, width, height, fps, kbps, maximum;
    int64_t pts;
} Context;
static void fail_jpeg(j_common_ptr jpeg) {
    Context *ctx = (Context *)jpeg->client_data;
    (*jpeg->err->output_message)(jpeg);
    longjmp(ctx->jump, 1);
}
static uint32_t number(const uint8_t *p) {
    return ((uint32_t)p[0]<<24)|((uint32_t)p[1]<<16)|((uint32_t)p[2]<<8)|p[3];
}
static void put_number(uint8_t *p, uint32_t n) {
    p[0]=(uint8_t)(n>>24); p[1]=(uint8_t)(n>>16); p[2]=(uint8_t)(n>>8); p[3]=(uint8_t)n;
}
static int write_all(const void *p, size_t size) { return fwrite(p,1,size,stdout)==size; }
/* Decode common 4:4:4 / 4:2:2 / 4:2:0 JPEGs without upsampling and
 * interleaving chroma first. Each iMCU uses bounded, padded component rows. */
static int raw_pixels(Context *ctx, int w, int h, int gray) {
    j_decompress_ptr d=&ctx->jpeg;
    JSAMPROW row[3][16]; JSAMPARRAY planes[3]={row[0],row[1],row[2]};
    for (int ci=0;ci<d->num_components;ci++) {
        size_t stride=(size_t)d->comp_info[ci].width_in_blocks*DCTSIZE;
        int count=d->comp_info[ci].v_samp_factor*DCTSIZE;
        if (count>16 || stride>(size_t)w+16) return 0;
        uint8_t *p=realloc(ctx->raw[ci],stride*(unsigned)count);
        if (!p) return 0;
        ctx->raw[ci]=p;
        for (int r=0;r<count;r++) row[ci][r]=p+stride*(unsigned)r;
    }
    int group=d->max_v_samp_factor*DCTSIZE;
    for (int y=0;y<h;y+=group) {
        if (jpeg_read_raw_data(d,planes,(JDIMENSION)group)!=(JDIMENSION)group) return 0;
        int valid=h-y<group?h-y:group;
        for (int r=0;r<valid;r++)
            memcpy(ctx->picture.img.plane[0]+(size_t)(y+r)*ctx->picture.img.i_stride[0],row[0][r],w);
        for (int ci=1;ci<=2;ci++) for (int r=0;r<valid/2;r++) {
            uint8_t *dst=ctx->picture.img.plane[ci]+(size_t)(y/2+r)*ctx->picture.img.i_stride[ci];
            if (gray) { memset(dst,128,w/2); continue; }
            int hs=d->max_h_samp_factor, vs=d->max_v_samp_factor;
            const uint8_t *a=row[ci][r*(2/vs)], *b=row[ci][r*(2/vs)+(vs==1)];
            if (hs==2 && vs==2) memcpy(dst,a,w/2);
            else if (hs==2) for (int x=0;x<w/2;x++) dst[x]=(uint8_t)(((unsigned)a[x]+b[x]+1)/2);
            else if (vs==2) for (int x=0;x<w/2;x++) dst[x]=(uint8_t)(((unsigned)a[2*x]+a[2*x+1]+1)/2);
            else for (int x=0;x<w/2;x++) dst[x]=(uint8_t)(((unsigned)a[2*x]+a[2*x+1]+b[2*x]+b[2*x+1]+2)/4);
        }
    }
    return 1;
}
static int decode(Context *ctx, uint32_t length, int fps, int kbps) {
    struct timespec started, finished; clock_gettime(CLOCK_MONOTONIC,&started);
    if (setjmp(ctx->jump)) return 0;
    ctx->jpeg.err=jpeg_std_error(&ctx->errors); ctx->errors.error_exit=fail_jpeg;
    ctx->jpeg.client_data=ctx;
    jpeg_create_decompress(&ctx->jpeg); ctx->created=1; ctx->jpeg.client_data=ctx;
    jpeg_mem_src(&ctx->jpeg,ctx->input,length);
    if (jpeg_read_header(&ctx->jpeg,TRUE)!=JPEG_HEADER_OK) return 0;
    int w=(int)ctx->jpeg.image_width, h=(int)ctx->jpeg.image_height;
    if (w<2 || h<2 || (w&1) || (h&1) || w>8192 || h>8192 || (uint64_t)w*h>MAX_PIXELS ||
        ctx->jpeg.progressive_mode) return 0;
    if (ctx->encoder && (w!=ctx->width || h!=ctx->height || fps!=ctx->fps)) return 0;
    if (!ctx->encoder) {
        if (!ctx->maximum) ctx->maximum=kbps;
        x264_param_t p;
        if (x264_param_default_preset(&p,"ultrafast","zerolatency")) return 0;
        p.i_width=w; p.i_height=h; p.i_csp=X264_CSP_I420;
        p.i_fps_num=fps; p.i_fps_den=1; p.i_timebase_num=1; p.i_timebase_den=fps;
        p.b_vfr_input=0; p.i_keyint_max=fps; p.i_keyint_min=fps; p.i_scenecut_threshold=0;
        p.b_repeat_headers=1; p.b_annexb=1; p.b_aud=1; p.i_bframe=0;
        p.vui.b_fullrange=1; p.i_log_level=X264_LOG_ERROR;
        /* Select the SPS level against the configured ceiling, even when the
         * first viewer starts low. Later raises must not exceed that level. */
        p.rc.i_rc_method=X264_RC_ABR; p.rc.i_bitrate=ctx->maximum;
        p.rc.i_vbv_max_bitrate=ctx->maximum;
        p.rc.i_vbv_buffer_size=ctx->maximum/2>128?ctx->maximum/2:128;
        if (x264_param_apply_profile(&p,"baseline")) return 0;
        ctx->encoder=x264_encoder_open(&p);
        if (!ctx->encoder) return 0;
        if (x264_picture_alloc(&ctx->picture,X264_CSP_I420,w,h)) return 0;
        ctx->allocated=1; ctx->width=w; ctx->height=h; ctx->fps=fps; ctx->kbps=ctx->maximum;
        ctx->rows=malloc((size_t)w*6);
        if (!ctx->rows) return 0;
    }
    int gray=ctx->jpeg.jpeg_color_space==JCS_GRAYSCALE;
    ctx->jpeg.out_color_space=gray?JCS_GRAYSCALE:JCS_YCbCr;
    ctx->jpeg.dct_method=JDCT_IFAST;
    int raw=gray || (ctx->jpeg.jpeg_color_space==JCS_YCbCr && ctx->jpeg.num_components==3 &&
        ctx->jpeg.max_h_samp_factor<=2 && ctx->jpeg.max_v_samp_factor<=2 &&
        ctx->jpeg.comp_info[0].h_samp_factor==ctx->jpeg.max_h_samp_factor &&
        ctx->jpeg.comp_info[0].v_samp_factor==ctx->jpeg.max_v_samp_factor &&
        ctx->jpeg.comp_info[1].h_samp_factor==1 && ctx->jpeg.comp_info[1].v_samp_factor==1 &&
        ctx->jpeg.comp_info[2].h_samp_factor==1 && ctx->jpeg.comp_info[2].v_samp_factor==1);
    ctx->jpeg.raw_data_out=raw;
    if (!jpeg_start_decompress(&ctx->jpeg)) return 0;
    if (raw) {
        if (!raw_pixels(ctx,w,h,gray)) return 0;
    } else {
    const int stride=gray?w:w*3;
    for (int y=0;y<h;y+=2) {
        JSAMPROW rows[2]={ctx->rows,ctx->rows+stride};
        for (int i=0;i<2;i++) if (jpeg_read_scanlines(&ctx->jpeg,&rows[i],1)!=1) return 0;
        uint8_t *a=ctx->picture.img.plane[0]+(size_t)y*ctx->picture.img.i_stride[0];
        uint8_t *b=a+ctx->picture.img.i_stride[0];
        uint8_t *u=ctx->picture.img.plane[1]+(size_t)(y/2)*ctx->picture.img.i_stride[1];
        uint8_t *v=ctx->picture.img.plane[2]+(size_t)(y/2)*ctx->picture.img.i_stride[2];
        if (gray) {
            memcpy(a,rows[0],w); memcpy(b,rows[1],w); memset(u,128,w/2); memset(v,128,w/2);
        } else for (int x=0;x<w;x+=2) {
            const uint8_t *r=rows[0]+x*3, *s=rows[1]+x*3;
            a[x]=r[0]; a[x+1]=r[3]; b[x]=s[0]; b[x+1]=s[3];
            u[x/2]=(uint8_t)(((unsigned)r[1]+r[4]+s[1]+s[4]+2)/4);
            v[x/2]=(uint8_t)(((unsigned)r[2]+r[5]+s[2]+s[5]+2)/4);
        }
    }
    }
    if (!jpeg_finish_decompress(&ctx->jpeg) || ctx->errors.num_warnings) return 0;
    jpeg_destroy_decompress(&ctx->jpeg); ctx->created=0;
    if (kbps!=ctx->kbps) {
        x264_param_t p; x264_encoder_parameters(ctx->encoder,&p);
        p.rc.i_bitrate=kbps; p.rc.i_vbv_max_bitrate=kbps;
        p.rc.i_vbv_buffer_size=kbps/2>128?kbps/2:128;
        if (x264_encoder_reconfig(ctx->encoder,&p)) return 0;
        ctx->kbps=kbps;
    }
    ctx->picture.i_pts=ctx->pts++; ctx->picture.i_type=X264_TYPE_AUTO;
    x264_picture_t out; x264_nal_t *nals=NULL; int count=0;
    int size=x264_encoder_encode(ctx->encoder,&nals,&count,&ctx->picture,&out);
    if (size<=0 || (unsigned)size>MAX_FRAME || out.i_pts!=ctx->picture.i_pts) return 0;
    clock_gettime(CLOCK_MONOTONIC,&finished);
    int64_t us=(finished.tv_sec-started.tv_sec)*1000000LL+(finished.tv_nsec-started.tv_nsec)/1000;
    uint8_t header[12]; put_number(header,(uint32_t)size); put_number(header+4,(uint32_t)ctx->kbps);
    put_number(header+8,(uint32_t)(us<0?0:us>UINT32_MAX?UINT32_MAX:us));
    if (!write_all(header,sizeof(header))) return 0;
    for (int i=0;i<count;i++) if (!write_all(nals[i].p_payload,(size_t)nals[i].i_payload)) return 0;
    return fflush(stdout)==0;
}
int main(int argc, char **argv) {
    Context *ctx=calloc(1,sizeof(*ctx)); if (!ctx) return 1;
    if (argc>2) { free(ctx); return 1; }
    if (argc==2) {
        char *end; long maximum=strtol(argv[1],&end,10);
        if (*end || maximum<1 || maximum>100000) { free(ctx); return 1; }
        ctx->maximum=(int)maximum;
    }
    int result=0; uint8_t header[16];
    for (;;) {
        size_t got=fread(header,1,sizeof(header),stdin);
        if (!got && feof(stdin)) break;
        if (got!=sizeof(header)) { result=1; break; }
        uint32_t kbps=number(header+4),fps=number(header+8),length=number(header+12);
        if (got!=sizeof(header) || memcmp(header,"WKH1",4) || !kbps || kbps>100000 || !fps || fps>120 ||
            length<4 || length>MAX_JPEG || (ctx->maximum && kbps>(unsigned)ctx->maximum)) { result=1; break; }
        uint8_t *input=realloc(ctx->input,length);
        if (!input) { result=1; break; } ctx->input=input;
        if (fread(input,1,length,stdin)!=length || !decode(ctx,length,(int)fps,(int)kbps)) { result=1; break; }
    }
    if (result) fputs("native H.264 input or encoding failed\n",stderr);
    if (ctx->created) jpeg_destroy_decompress(&ctx->jpeg);
    if (ctx->allocated) x264_picture_clean(&ctx->picture);
    if (ctx->encoder) x264_encoder_close(ctx->encoder);
    for (int i=0;i<3;i++) free(ctx->raw[i]);
    free(ctx->rows); free(ctx->input); free(ctx);
    return result;
}
