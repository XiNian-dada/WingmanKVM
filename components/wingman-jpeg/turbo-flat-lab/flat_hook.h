/* Experimental per-compressor hook for libjpeg-turbo 2.1.5, JPEG v6 ABI.
 * Copyright (c) 2026 WingmanKVM contributors. MIT license.
 * Uses the internal forward-DCT interface documented in jpegint.h 2.1.5.
 * Does not patch library code, files, other compressors, or the video service.
 * A source-level patch is preferable to this private-ABI hook for distribution.
 */
#ifndef WINGMAN_FLAT_HOOK_H
#define WINGMAN_FLAT_HOOK_H
#include "flat_core.h"

typedef void (*wk_forward_fn)(j_compress_ptr,jpeg_component_info *,JSAMPARRAY,
                              JBLOCKROW,JDIMENSION,JDIMENSION,JDIMENSION);
struct wk_fdct_layout {void (*start_pass)(j_compress_ptr);wk_forward_fn forward_DCT;};
typedef struct {
    wk_forward_fn original;void *controller;wk_flat_core core;
    uint64_t fallback_runs;int active;
} wk_flat_hook;

static void wk_forward(j_compress_ptr cinfo,jpeg_component_info *component,
                       JSAMPARRAY samples,JBLOCKROW coefficients,
                       JDIMENSION row,JDIMENSION column,JDIMENSION count) {
    wk_flat_hook *hook=(wk_flat_hook *)cinfo->client_data;
    int table=component->quant_tbl_no,ci=component->component_index;
    if(!wk_flat_begin(&hook->core,ci,table,count)) {
        hook->original(cinfo,component,samples,coefficients,row,column,count);return;
    }
    JDIMENSION run=0,run_start=0;
    for(JDIMENSION block=0;block<count;block++) {
        JCOEF dc;
        if(wk_flat_try(&hook->core,ci,table,samples,row,column+block*8,&dc)) {
            if(run){hook->original(cinfo,component,samples,coefficients+run_start,
                row,column+run_start*8,run);hook->fallback_runs++;run=0;}
            memset(coefficients[block],0,sizeof(JBLOCK));coefficients[block][0]=dc;
        } else {if(!run)run_start=block;run++;}
    }
    if(run){hook->original(cinfo,component,samples,coefficients+run_start,
        row,column+run_start*8,run);hook->fallback_runs++;}
}

static int wk_flat_attach(j_compress_ptr cinfo,wk_flat_hook *hook,int adaptive) {
#if JPEG_LIB_VERSION != 62 || BITS_IN_JSAMPLE != 8
    (void)cinfo;(void)hook;(void)adaptive;return 0;
#else
    if(!cinfo->fdct||cinfo->client_data||cinfo->dct_method!=JDCT_ISLOW||cinfo->data_precision!=8)return 0;
    if(cinfo->global_state!=101&&cinfo->global_state!=102)return 0;
    memset(hook,0,sizeof(*hook));hook->controller=cinfo->fdct;
    wk_flat_init(&hook->core,cinfo,adaptive);
    unsigned char *slot=(unsigned char *)cinfo->fdct+offsetof(struct wk_fdct_layout,forward_DCT);
    memcpy(&hook->original,slot,sizeof(hook->original));if(!hook->original)return 0;
    wk_forward_fn forward=wk_forward;cinfo->client_data=hook;
    memcpy(slot,&forward,sizeof(forward));hook->active=1;return 1;
#endif
}

/* Detach BEFORE jpeg_finish_compress: it can free the image pool. */
static void wk_flat_detach(j_compress_ptr cinfo,wk_flat_hook *hook) {
    if(!hook->active)return;
    if((cinfo->global_state==101||cinfo->global_state==102)&&cinfo->fdct==hook->controller) {
        unsigned char *slot=(unsigned char *)cinfo->fdct+offsetof(struct wk_fdct_layout,forward_DCT);
        memcpy(slot,&hook->original,sizeof(hook->original));
    }
    if(cinfo->client_data==hook)cinfo->client_data=NULL;
    hook->active=0;
}
#endif
