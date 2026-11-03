# 直接优化 libjpeg-turbo：独立实验

这是离线研究，不替换系统库、不挂钩生产进程、不接入 WingmanKVM 服务。先前 Rust 核心实验在复杂图像上仍明显落后于系统库；这里直接从 libjpeg-turbo 2.1.5 出发，寻找能保留成熟 SIMD 实现的局部改进。

## 精确纯色块快捷路径

对已完成色彩转换、采样的 8×8 分量块，完整检查 64 个样本是否相等。若都是 `s`，准确整数 DCT 的量化结果只有 DC：

```text
dc = (s - 128) * 8
quantized_dc = sign(dc) * floor((abs(dc) + floor(qdc/2)) / qdc)
all 63 AC coefficients = 0
```

直接查表写出结果，跳过 level shift、DCT 和其余量化。没有把近似纯色当作纯色，也不改变质量参数、Huffman、采样方式或 JPEG 字节。每个实际省略 DCT 的块都经过完整检查。

自适应策略按分量累计 64 个探测块，命中不足 1/8 时，接下来 960 个块走原版路径，随后重新探测。平滑渐变与噪声无需始终付出检查成本。它只影响速度，不影响输出；漏掉纯色块仍运行原版计算。最坏复杂度仍是 O(N)。

`flat_core.h` 为共享逻辑。`patch_source.py` 在**新目录**创建修改版，只修改 `jcdctmgr.c` 并加入该头文件；保留原始库及其许可证。仅在 8 位、`JDCT_ISLOW` 路径使用；浮点、快速整数和其他位深保留原路径。DC 量化值超过 255 的表回退。

`flat_hook.h` 是最初验证安装库的**私有 ABI 挂钩实验**，仅限 libjpeg-turbo 2.1.5、v6 ABI、8 位和空 `client_data`，不得视作通用稳定 API。它只操作本测试程序创建的 compressor，在 `finish_compress` 前解除。正式集成应采用源码补丁及稳定封装。

## RK3399 测量

2026-11-03，CPU 4 大核、最高 1.8 GHz、`nice -n 10`，governor 未变，后台服务继续运行。1920×1080 RGB→JPEG，质量 81、4:4:4、ISLOW、标准 Huffman，包含 compressor 创建/销毁，不包含解码、采集、网络或浏览器。预留输出缓冲，两轮预热、21 轮交替顺序；同一进程同时链接带不同符号前缀的原版/修改版库，每轮比较完整 JPEG 字节。

| 合成输入 | 原版源码构建 | 修改版源码构建 | 耗时变化 |
| --- | ---: | ---: | ---: |
| 纯色 | 40.119 ms | 23.639 ms | −41.1% |
| 稀疏终端 | 41.513 ms | 26.532 ms | −36.1% |
| 密集字符 | 51.219 ms | 48.111 ms | −6.1% |
| 渐变 | 43.861 ms | 43.960 ms | +0.2% |
| 噪声 | 81.051 ms | 81.492 ms | +0.5% |

两边为同一官方 2.1.5 源码、同一 GCC 9.2.0、相同 Release/SIMD 配置、静态 musl。**这张表不是拿自行构建版与 Debian 预编译系统库直接相减。** 系统安装的是 Debian `libjpeg62-turbo 1:2.1.5-4`；其另行测量的原版耗时约为 37.5/39.3/48.4/41.0/77.7 ms，编译器及 libc 不同。修改版在前两项也低于这些安装库数值，但可靠归因采用表中的同构建比较。

原始逐项日志、P95、输入 FNV64 和二进制 SHA-256 见 [benchmarks/rk3399-source.json](benchmarks/rk3399-source.json)。五种合成图只是诊断工作负载，未代表真实采集帧分布；亚百分比变化也不能在共享机器上认定为稳定收益或零回退。

源码对照测试 **67,710 个用例全部通过**：256 种样本 × 255 种 DC 量化值，奇数尺寸与补边，灰度/RGB/CMYK，纯色/末像素扰动/噪声，六档质量，4:4:4/4:2:2/4:2:0，标准/优化/渐进 Huffman，restart markers。安装库挂钩的两种模式也通过同一测试集。所有比较都是完整 JPEG 字节一致。

## 复现

官方源码：[libjpeg-turbo 2.1.5](https://github.com/libjpeg-turbo/libjpeg-turbo/releases/tag/2.1.5)。下载 tag 归档 `https://github.com/libjpeg-turbo/libjpeg-turbo/archive/refs/tags/2.1.5.tar.gz`，本次归档 SHA-256：

```text
254f3642b04e309fee775123133c6464181addc150499561020312ec61c1bf7c
```

已有 CMake、Python 3、GCC、binutils 的 ARM64 环境可运行：

```sh
python3 build_source.py /path/to/libjpeg-turbo-2.1.5 /new/task/build
nice -n 10 taskset -c 4 /new/task/build/source-test
nice -n 10 taskset -c 4 /new/task/build/source-compare sparse
# 另外运行 flat / desktop / gradient / noise
```

`--tool-prefix aarch64-linux-musl-` 允许使用已安装的交叉工具链。脚本不下载、安装或部署任何东西，输出目录必须不存在；构建并行度上限为 2。此次实际在云端容器交叉编译，容器仅挂载本次任务目录，关闭网络，限制 2 CPU、2 GiB，镜像为：

```text
ghcr.io/cross-rs/aarch64-unknown-linux-musl@sha256:702154f52b2d8091671aa2c84d5582d849f949977228c735ff8462f93cc0e1e4
```

容器内没有 Python 3；实际采用同样的 CMake/GCC 命令构建，由宿主 Python 运行补丁和符号映射脚本、容器执行 nm/objcopy。完整 `build_source.py` 尚未在这台容器中一键执行，其各构建阶段已实际完成。没有运行 `cmake --install`。

如只验证安装库，需匹配 ABI 的开发头文件：

```sh
cc -O3 -Wall -Wextra -Werror test.c -Wl,-l:libjpeg.so.62 -o flat-test
cc -O3 compare.c -Wl,-l:libjpeg.so.62 -o flat-compare
./flat-test
# 第三参数：0 原版、1 始终检查、2 自适应
./flat-compare sparse /tmp/experiment.jpg 2
```

板上没有开发头文件；本次安装库实验使用任务目录内的 2.1.2 公共头文件，并将其生成配置设为 v6 ABI 62 / 8 位 / memory source-destination，`jpeg_CreateCompress` 的版本及结构大小检查通过。源码双库实验则使用本次 2.1.5 构建生成的匹配头文件。没有修改全局头文件或系统包。

## 系数域重压缩

`requant.c` 单独研究 MJPEG 降质量时避免 IDCT、RGB 转换和 FDCT的路线；依靠库已有的系数读写 API，本身不是新发明的 JPEG 转码方法。它与上面的字节精确优化是不同实验，不能把收益相加。结果与限制见 [REQUANT.md](REQUANT.md)。

## 来源、许可与未完成项

自有实验代码为仓库 MIT 许可。生成的修改版仍受上游 IJG/BSD/zlib 等原有许可约束，不改写为单一 MIT。**This software is based in part on the work of the Independent JPEG Group.** 保留下载源码中的全部许可证和作者声明，不在本仓库复制整个 C 库。算法及接口依据：[forward DCT 源码](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/2.1.5/jcdctmgr.c)、[系数读写文档](https://github.com/libjpeg-turbo/libjpeg-turbo/blob/2.1.5/libjpeg.txt)。

尚未验证最新版上游、全部架构、恶意输入隔离、长期运行、真实视频帧分布、硬件 JPEG 编码器或主服务集成。未发布、推送或替换任何生产库。这是针对目标工作负载的可复现实验，不宣称通用库已被全面超越。
