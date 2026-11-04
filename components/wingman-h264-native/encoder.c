/* GPL-2.0-or-later. Standalone encoder; Rust communicates only over pipes.
 * x264 is pinned in vendor/. JPEG errors never cross a language boundary. */
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
    uint8_t *input, *rows;
    int created, allocated, width, height, fps, kbps;
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
static int decode(Context *ctx, uint32_t length, int fps, int kbps) {
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
        x264_param_t p;
        if (x264_param_default_preset(&p,"ultrafast","zerolatency")) return 0;
        p.i_width=w; p.i_height=h; p.i_csp=X264_CSP_I420;
        p.i_fps_num=fps; p.i_fps_den=1; p.i_timebase_num=1; p.i_timebase_den=fps;
        p.b_vfr_input=0; p.i_keyint_max=fps; p.i_keyint_min=fps; p.i_scenecut_threshold=0;
        p.b_repeat_headers=1; p.b_annexb=1; p.b_aud=1; p.i_bframe=0;
        p.vui.b_fullrange=1; p.i_log_level=X264_LOG_ERROR;
        p.rc.i_rc_method=X264_RC_ABR; p.rc.i_bitrate=kbps;
        p.rc.i_vbv_max_bitrate=kbps; p.rc.i_vbv_buffer_size=kbps/2>128?kbps/2:128;
        if (x264_param_apply_profile(&p,"baseline")) return 0;
        ctx->encoder=x264_encoder_open(&p);
        if (!ctx->encoder) return 0;
        if (x264_picture_alloc(&ctx->picture,X264_CSP_I420,w,h)) return 0;
        ctx->allocated=1; ctx->width=w; ctx->height=h; ctx->fps=fps; ctx->kbps=kbps;
        ctx->rows=malloc((size_t)w*6);
        if (!ctx->rows) return 0;
    }
    int gray=ctx->jpeg.jpeg_color_space==JCS_GRAYSCALE;
    ctx->jpeg.out_color_space=gray?JCS_GRAYSCALE:JCS_YCbCr;
    ctx->jpeg.dct_method=JDCT_IFAST;
    if (!jpeg_start_decompress(&ctx->jpeg)) return 0;
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
    uint8_t header[8]; put_number(header,(uint32_t)size); put_number(header+4,(uint32_t)ctx->kbps);
    if (!write_all(header,sizeof(header))) return 0;
    for (int i=0;i<count;i++) if (!write_all(nals[i].p_payload,(size_t)nals[i].i_payload)) return 0;
    return fflush(stdout)==0;
}
int main(void) {
    Context *ctx=calloc(1,sizeof(*ctx)); if (!ctx) return 1;
    int result=0; uint8_t header[16];
    for (;;) {
        size_t got=fread(header,1,sizeof(header),stdin);
        if (!got && feof(stdin)) break;
        if (got!=sizeof(header)) { result=1; break; }
        uint32_t kbps=number(header+4),fps=number(header+8),length=number(header+12);
        if (got!=sizeof(header) || memcmp(header,"WKH1",4) || !kbps || kbps>100000 || !fps || fps>120 ||
            length<4 || length>MAX_JPEG) { result=1; break; }
        uint8_t *input=realloc(ctx->input,length);
        if (!input) { result=1; break; } ctx->input=input;
        if (fread(input,1,length,stdin)!=length || !decode(ctx,length,(int)fps,(int)kbps)) { result=1; break; }
    }
    if (result) fputs("native H.264 input or encoding failed\n",stderr);
    if (ctx->created) jpeg_destroy_decompress(&ctx->jpeg);
    if (ctx->allocated) x264_picture_clean(&ctx->picture);
    if (ctx->encoder) x264_encoder_close(ctx->encoder);
    free(ctx->rows); free(ctx->input); free(ctx);
    return result;
}
