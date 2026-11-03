#!/usr/bin/env python3
"""Create a separate patched libjpeg-turbo 2.1.5 tree; never edit the source tree."""
import argparse
import difflib
from pathlib import Path
import shutil

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("source", type=Path)
p.add_argument("destination", type=Path)
args = p.parse_args()
if args.source.resolve() in args.destination.resolve().parents:
    p.error("destination must be outside the source tree")
if args.destination.exists():
    p.error("destination must not already exist")
original = (args.source / "jcdctmgr.c").read_text()
text = original

def replace(old, new):
    global text
    if text.count(old) != 1:
        raise ValueError("unexpected upstream source: " + old[:80])
    text = text.replace(old, new)

replace('#include "jsimddct.h"', '''#include "jsimddct.h"
/* Modified by WingmanKVM contributors, 2026: experimental exact flat-block
 * shortcut. Original libjpeg-turbo licenses and notices remain applicable. */
#if BITS_IN_JSAMPLE == 8
#include "flat_core.h"
#endif''')
replace('  struct jpeg_forward_dct pub;  /* public fields */', '''  struct jpeg_forward_dct pub;  /* public fields */
#if BITS_IN_JSAMPLE == 8
  wk_flat_core wk_flat;
#endif''')
replace('''  quantize_method_ptr do_quantize = fdct->quantize;
  workspace = fdct->workspace;

  sample_data += start_row;     /* fold in the vertical offset once */''', '''  quantize_method_ptr do_quantize = fdct->quantize;
  workspace = fdct->workspace;

  sample_data += start_row;     /* fold in the vertical offset once */

#if BITS_IN_JSAMPLE == 8
  if (cinfo->dct_method == JDCT_ISLOW &&
      wk_flat_begin(&fdct->wk_flat, compptr->component_index,
                    compptr->quant_tbl_no, num_blocks)) {
    for (bi = 0; bi < num_blocks; bi++, start_col += DCTSIZE) {
      JCOEF dc;
      if (wk_flat_try(&fdct->wk_flat, compptr->component_index,
                      compptr->quant_tbl_no, sample_data, 0, start_col, &dc)) {
        memset(coef_blocks[bi], 0, sizeof(JBLOCK));
        coef_blocks[bi][0] = dc;
      } else {
        (*do_convsamp) (sample_data, start_col, workspace);
        (*do_dct) (workspace);
        (*do_quantize) (coef_blocks[bi], divisors, workspace);
      }
    }
    return;
  }
#endif''')
replace('  fdct->pub.start_pass = start_pass_fdctmgr;', '''  fdct->pub.start_pass = start_pass_fdctmgr;
#if BITS_IN_JSAMPLE == 8
  wk_flat_init(&fdct->wk_flat, cinfo, 1);
#endif''')
shutil.copytree(args.source, args.destination)
(args.destination / "jcdctmgr.c").write_text(text)
shutil.copyfile(Path(__file__).with_name("flat_core.h"), args.destination / "flat_core.h")
print("".join(difflib.unified_diff(original.splitlines(True), text.splitlines(True),
                                fromfile="a/jcdctmgr.c", tofile="b/jcdctmgr.c")), end="")
