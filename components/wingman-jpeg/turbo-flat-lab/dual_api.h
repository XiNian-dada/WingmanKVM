/* Link both archives after prefixing all their defined global symbols. */
#ifndef WINGMAN_DUAL_API_H
#define WINGMAN_DUAL_API_H
#include <jpeglib.h>
#define WK_API_LIST(X) \
    X(jpeg_std_error) X(jpeg_CreateCompress) X(jpeg_mem_dest) \
    X(jpeg_set_defaults) X(jpeg_set_quality) X(jpeg_simple_progression) \
    X(jpeg_start_compress) X(jpeg_write_scanlines) X(jpeg_finish_compress) \
    X(jpeg_destroy_compress)
#define WK_DECLARE(name) extern __typeof__(name) stock_##name; extern __typeof__(name) flat_##name;
WK_API_LIST(WK_DECLARE)
#undef WK_DECLARE
#define WK_CALL(name) (mode ? flat_##name : stock_##name)
#endif
