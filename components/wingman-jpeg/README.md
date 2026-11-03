# wingman-jpeg（实验组件）

面向连续 JPEG 视频帧的 Rust 组件。ARM64 上使用自有 NEON RGB→YCbCr 转换，复用解码输出缓冲，并提供直接解码 JPEG 亮度的灰度路径。

这是基于现有编解码器公共接口的优化组件：`jpeg-encoder 0.6.1` 负责 DCT、量化、Huffman 编码和 JPEG 文件结构，`zune-jpeg 0.5.15` 负责解码。没有修改或复制这些库的完整实现。发布方式尚未决定，暂设 `publish = false`，也尚未替换 WingmanKVM 的生产链路。

## 优化边界

- **彩色编码**：每行预分配输出位置，以 NEON 一次转换 8 个 RGB 像素。系数与舍入规则对齐上游，尾部由标量处理。始终保留 4:4:4，测试要求 JPEG 字节与未修改的上游编码器一致。
- **彩色重压缩**：从借用的 JPEG 字节直接解码，避免 `image` 包装路径的输入复制和重复头部处理；复用像素输出缓冲。
- **保持原灰度定义**：`GrayRgb` 使用与 `image 0.25.10` 相同的 sRGB 派生亮度系数、浮点展开、FMA 选择和舍入规则，并复用 RGB 和灰度缓冲，要求 JPEG 字节不变。
- **直接灰度**：请求解码器输出 JPEG 的亮度分量，省去 RGB 输出与 RGB→灰度转换。对于 YCbCr 源，它与“先转 RGB 再转灰度”存在亮度系数（BT.601 与 BT.709 风格权重）、舍入、色彩裁剪等差异，不能声称是逐像素相同的优化。基准单独报告灰度差异。
- **可回退**：`Backend::Scalar` 显式选择可移植参考路径；`Backend::Auto` 在 AArch64 使用 NEON，其他架构使用可移植路径。没有线程池、外部进程、系统库或网络访问。

## 使用

当前通过本地路径引入；Rust 最低版本为 1.88。

```toml
[dependencies]
wingman-jpeg = { path = "components/wingman-jpeg" }
```

```rust
use wingman_jpeg::{Backend, ColorMode, Transcoder, encode_rgb_into};

# fn example(jpeg: &[u8], rgb: &[u8]) -> Result<(), wingman_jpeg::Error> {
let mut output = Vec::new();
encode_rgb_into(rgb, 1920, 1080, 81, Backend::Auto, &mut output)?;

let mut transcoder = Transcoder::default(); // 连续帧复用同一实例
transcoder.transcode_into(jpeg, 81, ColorMode::Color, Backend::Auto, &mut output)?;
transcoder.transcode_into(jpeg, 81, ColorMode::GrayRgb, Backend::Auto, &mut output)?;
// 另选 JPEG 亮度定义；与 GrayRgb 的亮度可能不同。
transcoder.transcode_into(jpeg, 81, ColorMode::JpegLuma, Backend::Auto, &mut output)?;
# Ok(())
# }
```

输入是紧密排列的 RGB8 或 JPEG，输出为标准 baseline JPEG。`quality` 限制在 1–100；输入/输出别名由 Rust 借用规则禁止。输出缓冲在调用开始时清空，出错时调用方应丢弃当次输出。对象由调用方独占，不在内部排队。

对 JPEG 输入限制压缩数据为 32 MiB、单边 8192 像素、总像素 33,177,600、渐进扫描最多 64 次。这些是视频用途的尺寸限制，不是进程总内存预算。ICC、EXIF、旋转方向等元数据不保留；面向采集卡帧，不作为通用照片归档工具。

## 验证和基准

在本目录运行：

```sh
cargo test --locked --release
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked --release --example compare -- desktop 15 1920 1080
cargo run --locked --release --example compare -- gradient 15 1920 1080
cargo run --locked --release --example compare -- noise 15 1920 1080
```

基准使用同一进程、同一组输入、相同画质与 4:4:4 输出。每种路径先预热一次，再轮换执行顺序；报告每帧中位数、P95、最小值、输出体积，并对彩色和 GrayRgb 输出做字节一致性断言。包含未修改上游编码器、自有标量行处理、NEON、原始 `image` 编码路径、目前 WingmanKVM 路径、优化重压缩路径和三种灰度路径。

测试覆盖全部 16,777,216 种 RGB 颜色、不对齐输入、尾部像素、奇数尺寸、不同画质、完整 JPEG 字节一致性、灰度输入、渐进 JPEG、尺寸变化、错误输入及大尺寸头部限制。ARM 内核必须在真实 AArch64 机器上运行这些测试，x86 测试不会执行 NEON。

性能实测及限制见 [BENCHMARKS.md](BENCHMARKS.md)。

## 许可与来源

自有代码为 MIT，见 [LICENSE](LICENSE)。第三方依赖保留各自许可与作者署名，见 [THIRD_PARTY.md](THIRD_PARTY.md)。本组件不声称拥有上游 JPEG 算法或库。
