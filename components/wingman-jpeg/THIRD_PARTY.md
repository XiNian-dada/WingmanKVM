# 第三方来源

本组件直接依赖下列未修改的 crates.io 包：

| 包 | 固定版本 | 上游 | 许可 |
| --- | --- | --- | --- |
| jpeg-encoder | 0.6.1 | https://github.com/vstroebel/jpeg-encoder/tree/v0.6.1 | (MIT OR Apache-2.0) AND IJG |
| zune-jpeg | 0.5.15 | https://github.com/etemesi254/zune-image | MIT OR Apache-2.0 OR Zlib |
| moxcms | 0.8.1 | https://github.com/awxkee/moxcms | BSD-3-Clause OR Apache-2.0 |
| zune-core | 0.5.1 | https://github.com/etemesi254/zune-image | MIT OR Apache-2.0 OR Zlib |

`image 0.25.10` 仅用于测试及基准，许可 MIT OR Apache-2.0；来源 https://github.com/image-rs/image 。

NEON 转换使用与 `jpeg_encoder::rgb_to_ycbcr` 相同的公开固定点系数及舍入定义以保持字节一致性；DCT、量化、熵编码、JPEG 解析与解码仍由上游完成。库的许可证不覆盖第三方依赖；对二进制做正式分发时应随附所选择的依赖许可及必要声明，包括 IJG 声明。

固定版本和校验和见 Cargo.lock。没有将修改版上游代码伪装为未修改的原版，也没有保留未审查的源码补丁。
