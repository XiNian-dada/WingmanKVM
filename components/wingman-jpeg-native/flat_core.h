/* Shared exact flat-block logic. MIT, WingmanKVM contributors, 2026. */
#ifndef WINGMAN_FLAT_CORE_H
#define WINGMAN_FLAT_CORE_H
#include <stdio.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
/* This header is copied beside the vendored jpeglib.h, never use host headers. */
#include "jpeglib.h"
#if defined(__aarch64__)
#include <arm_neon.h>
#endif

typedef struct {
    JCOEF dc[NUM_QUANT_TBLS][256];
    unsigned char eligible[NUM_QUANT_TBLS];
    unsigned tested[10],hits[10],cooldown[10];
    uint64_t blocks,flat_blocks,bypassed_blocks;
    int adaptive;
} wk_flat_core;

static int wk_is_flat(JSAMPARRAY rows, JDIMENSION row, JDIMENSION column,
                      JSAMPLE *value) {
    const JSAMPLE sample=rows[row][column];
#if defined(__aarch64__)
    uint8x8_t expected=vdup_n_u8(sample);
    uint8x8_t same=vceq_u8(vld1_u8(rows[row]+column),expected);
    /* Noise and most textured tiles exit after one row. */
    if(vminv_u8(same)!=255)return 0;
    for(unsigned y=1;y<8;y++)same=vand_u8(same,vceq_u8(vld1_u8(rows[row+y]+column),expected));
    if(vminv_u8(same)!=255)return 0;
#else
    for(unsigned y=0;y<8;y++)for(unsigned x=0;x<8;x++)
        if(rows[row+y][column+x]!=sample)return 0;
#endif
    *value=sample;return 1;
}

static void wk_flat_init(wk_flat_core *state,j_compress_ptr cinfo,int adaptive) {
    memset(state,0,sizeof(*state));state->adaptive=adaptive;
    for(int table=0;table<NUM_QUANT_TBLS;table++) {
        if(!cinfo->quant_tbl_ptrs[table])continue;
        unsigned q=cinfo->quant_tbl_ptrs[table]->quantval[0];
        if(!q||q>255)continue;
        state->eligible[table]=1;
        for(unsigned sample=0;sample<256;sample++) {
            int dc=((int)sample-128)*8;
            unsigned magnitude=(unsigned)(dc<0?-dc:dc);
            int quantized=(int)((magnitude+q/2)/q);
            state->dc[table][sample]=(JCOEF)(dc<0?-quantized:quantized);
        }
    }
}

/* Returns false for an entire cooldown batch, leaving the original DCT loop
 * untouched. A 64-block probe followed by 960 skipped blocks bounds sampling
 * overhead on images with no exact-flat tiles. False negatives only cost speed. */
static int wk_flat_begin(wk_flat_core *state,int component,int table,JDIMENSION count) {
    if(component<0||component>=10||table<0||table>=NUM_QUANT_TBLS||!state->eligible[table])return 0;
    state->blocks+=count;
    if(state->adaptive&&state->cooldown[component]>=count) {
        state->cooldown[component]-=count;state->bypassed_blocks+=count;return 0;
    }
    return 1;
}

static int wk_flat_try(wk_flat_core *state,int component,int table,JSAMPARRAY samples,
                       JDIMENSION row,JDIMENSION column,JCOEF *dc) {
    if(state->adaptive&&state->cooldown[component]) {
        state->cooldown[component]--;state->bypassed_blocks++;return 0;
    }
    JSAMPLE sample;int flat=wk_is_flat(samples,row,column,&sample);
    if(state->adaptive) {
        state->tested[component]++;state->hits[component]+=flat;
        if(state->tested[component]==64) {
            if(state->hits[component]*8<state->tested[component])state->cooldown[component]=960;
            state->tested[component]=state->hits[component]=0;
        }
    }
    if(flat){*dc=state->dc[table][sample];state->flat_blocks++;}
    return flat;
}
#endif
