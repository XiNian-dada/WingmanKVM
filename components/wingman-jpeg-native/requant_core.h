/* Exact baseline coarsening only. MIT, WingmanKVM contributors, 2026. */
#ifndef WINGMAN_REQUANT_CORE_H
#define WINGMAN_REQUANT_CORE_H
#include <stdint.h>
#include <jpeglib.h>
#if defined(__aarch64__)
#include <arm_neon.h>
#endif

typedef struct {
    uint16_t oldq[64],newq[64];
    int32_t reciprocal31[64];
} wk_requant_table;

static void wk_requant_init(wk_requant_table *table,const unsigned *oldq,const unsigned *newq) {
    for(int k=0;k<64;k++) {
        table->oldq[k]=(uint16_t)oldq[k];table->newq[k]=(uint16_t)newq[k];
        table->reciprocal31[k]=newq[k]==1?0:(int32_t)((UINT64_C(0x80000000)+newq[k]-1)/newq[k]);
    }
}
static int wk_zero_ac(const JCOEF *block) {
#if defined(__aarch64__)
    uint16x8_t any=vsetq_lane_u16(0,vreinterpretq_u16_s16(vld1q_s16(block)),0);
    for(int k=8;k<64;k+=8)any=vorrq_u16(any,vreinterpretq_u16_s16(vld1q_s16(block+k)));
    return vmaxvq_u16(any)==0;
#else
    for(int k=1;k<64;k++)if(block[k])return 0;
    return 1;
#endif
}
#if defined(__aarch64__)
static uint32x4_t wk_requant_four(uint16x4_t mag,uint16x4_t oldq,uint16x4_t newq,int32x4_t r) {
    uint32x4_t divisor=vmovl_u16(newq);
    uint32x4_t n=vaddq_u32(vmull_u16(mag,oldq),vshrq_n_u32(divisor,1));
    /* n < 2^24, r=ceil(2^31/d). SQDMULH's estimate is exact or one too high.
     * All operands are nonnegative signed i32. No saturating case is reachable.
     * d==1 uses an explicit identity lane because 2^31 is not positive i32. */
    uint32x4_t q=vreinterpretq_u32_s32(vqdmulhq_s32(vreinterpretq_s32_u32(n),r));
    q=vbslq_u32(vceqq_u32(divisor,vdupq_n_u32(1)),n,q);
    uint32x4_t over=vcgtq_u32(vmulq_u32(q,divisor),n);
    return vsubq_u32(q,vandq_u32(over,vdupq_n_u32(1)));
}
#endif
static void wk_requant_vector(JCOEF *block,const wk_requant_table *table) {
#if defined(__aarch64__)
    for(int k=0;k<64;k+=8) {
        int16x8_t value=vld1q_s16(block+k);
        uint16x8_t mag=vreinterpretq_u16_s16(vabsq_s16(value));
        uint16x8_t oldq=vld1q_u16(table->oldq+k),newq=vld1q_u16(table->newq+k);
        uint32x4_t low=wk_requant_four(vget_low_u16(mag),vget_low_u16(oldq),vget_low_u16(newq),vld1q_s32(table->reciprocal31+k));
        uint32x4_t high=wk_requant_four(vget_high_u16(mag),vget_high_u16(oldq),vget_high_u16(newq),vld1q_s32(table->reciprocal31+k+4));
        int16x8_t result=vreinterpretq_s16_u16(vcombine_u16(vmovn_u32(low),vmovn_u32(high)));
        result=vbslq_s16(vcltq_s16(value,vdupq_n_s16(0)),vnegq_s16(result),result);
        vst1q_s16(block+k,result);
    }
#else
    for(int k=0;k<64;k++) {
        int v=block[k];unsigned mag=(unsigned)(v<0?-v:v);
        unsigned q=(mag*table->oldq[k]+table->newq[k]/2)/table->newq[k];
        block[k]=(JCOEF)(v<0?-(int)q:(int)q);
    }
#endif
}
#endif
