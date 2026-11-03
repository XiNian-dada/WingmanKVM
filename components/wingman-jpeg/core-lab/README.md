# JPEG core-lab：位精确的 ARM64 内核实验

这是从 `jpeg-encoder 0.6.1` 复制出来的内核，研究整数 DCT、量化和分块取数。经差分验证的内核现作为 WingmanKVM 的可选“整数向量优化”后端；上一级 `wingman-jpeg` API 仍独立使用未修改的上游编码器。本包未独立发布，历史基准保留当时的集成状态。

## 做了什么

1. **NEON 整数 DCT**：把原版 LL&M 因式分解安排到四个向量通道，保持原来的常数、两轮缩放、舍入点及输出系数定义。
2. **Q15 向量量化**：利用 `SQDMULH` 一次量化八个系数。保留原版倒数与修正值，精确处理 `i16::MIN` 等边界，随后按原版 zigzag 排列。
3. **纯色块融合快捷路径**：当 8×8 块的 64 个样本全部相等时，直接生成 DC 并量化它，63 个 AC 系数直接置零，跳过整块 DCT 及 AC 量化。检查完整块，不把近似纯色误当作纯色。
4. **SIMD 分块取数**：连续采样的块用向量加载和减 128 完成 level shift；带采样步长的块保留原版路径。逐行检查边界后才读取，不越界试探。

颜色采样、JPEG 画质参数、Huffman 表、元数据写入及输出格式保持原有行为。目标是输出 JPEG **逐字节一致**，而不是降低画质换速度。

`DctBackend` 用于消融比较：

| 模式 | DCT | 量化 | 纯色融合 | SIMD 取块 |
| --- | --- | --- | --- | --- |
| Reference | 原版 | 原版 | 否 | 否 |
| Neon | NEON | 原版 | 否 | 否 |
| NeonFlat | NEON + 纯色 DCT 快捷路径 | 原版 | 否 | 否 |
| NeonQuant | NEON | Q15 NEON | 否 | 否 |
| NeonFlatQuant | NEON + 纯色 DCT 快捷路径 | Q15 NEON | 否 | 否 |
| FusedFlatQuant | NEON | Q15 NEON | 是 | 否 |
| FusedLoad / Auto（ARM64） | NEON | Q15 NEON | 是 | 是 |

不支持这些 ARM 内核的目标使用平台默认路径。原来的可选 x86 AVX2 实现保留，默认未启用。`benchmark` 特性只用于诊断实验，不是稳定 API。

## 数学依据及边界

**纯色块。** 输入为 level-shift 后的 `c ∈ [-128,127]` 时，原版第一轮仅有 DC=`32c`；第二轮在原版八倍缩放域中得到 DC=`64c`，其余系数为零。该除法整除，没有近似。使用原来的量化函数处理唯一 DC，输出等价。

**Q15。** 原版量化的幅值为 `floor((abs(v)+correction)*reciprocal/32768)`。把无符号和 `s` 重解释为有符号 `u` 时，若 `s≥32768`，有 `s=u+65536`，故结果恰为 `floor(u*r/32768)+2r`。这使八路 `SQDMULH` 可处理完整 i16 输入域，不必丢掉高位。接受的量化表满足 `r≤4096`、`correction≤1020`，中间和不超过 33788，不会使该指令发生饱和。

**DCT 位宽。** [仿射边界分析](scripts/verify_dct_bounds.py)对同一 LL&M 数学模型保留变量间抵消关系，检查八位样本下的所有中间表达式。最大 i32 幅值为 342,638,400，最终系数界为 `[-8192,8160]`，适合原版的 i16 存储。它不是对整个 Rust 程序的形式化证明；索引、转置、舍入及内存实现另由真机差分测试验证。

对于任意输入，读取样本和生成 JPEG 的整体最坏复杂度仍为 **O(N)**。这里改善的是每块工作量、硬件并行度，以及纯色结构下的执行路径，没有声称突破已经证明的下界，也没有声称发明了新的 DCT 因式分解。

## 复现

```sh
cargo test --locked --release
cargo check --locked --lib --no-default-features
cargo build --locked --release --features benchmark --example deep_compare
python3 scripts/verify_dct_bounds.py
# 在 RK3399 上绑定一个大核；按机器实际情况调整编号
nice -n 10 taskset -c 4 target/release/examples/deep_compare micro
nice -n 10 taskset -c 4 target/release/examples/deep_compare sparse 15
nice -n 10 taskset -c 4 target/release/examples/deep_compare desktop 15
nice -n 10 taskset -c 4 target/release/examples/deep_compare gradient 15
nice -n 10 taskset -c 4 target/release/examples/deep_compare noise 15
```

也提供 `flat` 最佳情形，以及末尾可选的 JPEG 输出目录参数。最终 release 配置为 ThinLTO、单 codegen unit，与主应用一致；同一二进制内包含未修改上游、上一阶段组件、参考内核及各优化组合，轮换运行并断言所有 Rust 路径字节相同。

[`examples/turbo_compare.c`](examples/turbo_compare.c)是独立的系统 libjpeg-turbo 参考程序，使用相同 RGB 输入、质量 81、4:4:4、准确整数 DCT及标准 Huffman 表，预留输出缓冲。它需要与系统库 ABI 匹配的开发头文件；不链接进本 Rust 组件，也不要求修改系统包。JPEG 字节可能不同，不能混入 Rust 的逐字节等价结论。

真机测量、限制、系统库对照见 [BENCHMARKS.md](BENCHMARKS.md)。

## 来源与修改说明

基线为 crates.io `jpeg-encoder 0.6.1`，包校验和 `b454d911ac55068f53495488d8ccd0646eaa540c033a28ee15b07838afafb01f`，上游为 <https://github.com/vstroebel/jpeg-encoder/tree/v0.6.1>。

保留原作者、原文档 [UPSTREAM_README.md](UPSTREAM_README.md)、原包清单 [UPSTREAM_Cargo.toml](UPSTREAM_Cargo.toml)、[MIT](LICENSE-MIT)、[Apache-2.0](LICENSE-APACHE)及 [IJG 声明](LICENSE-IJG)。**This software is based in part on the work of the Independent JPEG Group.** 原版 DCT 来自 IJG/mozjpeg 与 LL&M 算法；保留在未修改的 `src/fdct.rs` 中。

相对上游只修改三个既有源文件：`src/lib.rs`（ARM 模块和诊断导出）、`src/encoder.rs`（实现选择和融合调用点）、`src/quantization.rs`（紧凑的 ARM 表）。新增内核位于 `src/neon_dct.rs`、`src/neon_quant.rs`，诊断位于 `src/diagnostics.rs`。其余上游源文件逐字节保留，校验和见 [UPSTREAM_SHA256.json](UPSTREAM_SHA256.json)，可运行 `python3 scripts/verify_upstream.py <上游源码路径>` 核验。

本实验 `publish=false`。当前 Cargo 包还通过本地路径依赖上级研究组件；具体拆包、许可声明汇总、CI 接入与发布方式待决定。
