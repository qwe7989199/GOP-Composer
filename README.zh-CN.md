# GOP Composer

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20first-8ab4f8.svg)](#平台状态)
[![Codecs](https://img.shields.io/badge/codecs-H.264%20%7C%20HEVC-7bc8a4.svg)](#兼容性)

GOP Composer 是一个本地桌面工具，用于对分发视频码流做 GOP 级画面替换和重新封装。
它面向已经完成交付编码的 H.264/HEVC 文件：当只需要修正一小段画面时，不应该重新
编码整条节目。

[English README](README.md)

## 一眼看懂

| 维度 | 范围 |
| --- | --- |
| 主要用途 | 替换已有分发码流中的少量 GOP |
| 处理模型 | 复制压缩包，并按主轨道顺序重新封装 |
| 目标编码 | H.264 和 HEVC |
| 主要输出 | MP4 和 H.264/HEVC Annex B 裸流 |
| 隐私模型 | 全本地处理 |
| 当前平台重点 | Windows 桌面 |

## 为什么做这个工具

视频制作流程经常在完整输出后才发现很小的问题：烧录字幕的一句错译、一个坏镜头、
一个错误 logo，或者一小段图卡需要替换。专业母版流程可以用 IMF 这类增量包解决
类似问题，但分发端交付的通常还是普通 MP4、MKV 或 MOV 文件。

GOP Composer 解决的就是这个空档。

如果受影响内容只落在一个 GOP，或者连续几个 GOP 内，目标流程是：

1. 在分发码流里定位受影响 GOP。
2. 只用兼容编码参数重新压制对应帧范围。
3. 把原始分发文件作为主视频导入。
4. 导入替换素材，并把替换 GOP 放到主轨道。
5. 通过复制压缩包导出新的 MP4 或 H.264/HEVC 裸流。

未变化的 GOP 保持压缩包形态，不会被解码后重新编码。

## 功能

- 本地优先桌面 GUI。媒体文件只在本机处理，不上传。
- 通过 FFmpeg 为常见分发容器建立 GOP 索引。
- 围绕 H.264 和 HEVC 设计预览、校验、裸流导出和 MP4 重新封装流程。
- 多来源时间线，支持 GOP 选择、复制、剪切、粘贴、删除、撤销、重做和跨轨道插入。
- 基于主轨道导出：最终视频永远由当前放置在主轨道上的 GOP 顺序组成。
- MP4 导出支持时间戳重写、可选生成 PTS、可配置 timebase，以及从主轨道时间范围复制音频。
- H.264/HEVC Annex B 裸流导出，适合纯视频流程。
- 时间码辅助替换，可以把替换文件中的 GOP 快速应用到主轨道对应位置。
- 英文和中文界面，可在工具栏一键切换。

## 典型流程

1. 点击 **Open Main Video** 导入分发文件。
2. 点击 **Add Videos** 导入一个或多个替换文件。
3. 选中替换轨道上的 GOP，拖入主轨道；或者使用时间码自动应用匹配 GOP。
4. 检查 GOP 边界和替换点附近的预览画面。
5. 把主轨道导出为 MP4 或 H.264/HEVC 裸流。

GOP Composer 不会试图掩盖流兼容问题。替换素材应该是有意按照目标分发码流编码出来的。

## 时间码替换

如果替换文件包含可读取的时间码，GOP Composer 可以用绝对时间码帧把替换 GOP 映射到
主轨道 GOP：

```text
绝对帧 = 时间码起始帧 + GOP 起始帧
```

如果替换文件和主视频的时间码帧率一致，程序会询问是否把匹配到的替换 GOP 应用到
主轨道。被匹配的 GOP 会从替换轨道移动到主轨道对应位置。

这个功能只做机械映射。替换片段的帧范围、GOP 边界和画面内容是否正确，仍然由用户
自己保证。

### 用 FFmpeg 写入时间码

如果替换片段已经按正确参数编码好，只需要补时间码 metadata，可以直接重新封装：

```powershell
ffmpeg -y -i "replacement_no_tc.mp4" -map 0 -c copy `
  -timecode "01:00:04:00" `
  -metadata:s:v:0 timecode="01:00:04:00" `
  "replacement_tc.mp4"
```

如果替换片段是在这一步重新压制，可以在编码时写入时间码。这里的时间码应该等于第一个
替换 GOP 要对应的主轨道 GOP 起始时间码。

H.264 示例：

```powershell
ffmpeg -y -i "source_master.mov" -ss 00:12:34.000 -t 2.000 `
  -vf "subtitles=subtitle_fix.ass" -an `
  -c:v libx264 -pix_fmt yuv420p -r 24000/1001 `
  -g 24 -keyint_min 24 -x264-params "scenecut=0:open-gop=0" `
  -video_track_timescale 24000 `
  -timecode "01:00:04:00" `
  -metadata:s:v:0 timecode="01:00:04:00" `
  "replacement_tc.mp4"
```

HEVC 示例：

```powershell
ffmpeg -y -i "source_master.mov" -ss 00:12:34.000 -t 2.000 `
  -vf "subtitles=subtitle_fix.ass" -an `
  -c:v libx265 -pix_fmt yuv420p -r 24000/1001 `
  -x265-params "keyint=24:min-keyint=24:scenecut=0:open-gop=0" `
  -video_track_timescale 24000 `
  -timecode "01:00:04:00" `
  -metadata:s:v:0 timecode="01:00:04:00" `
  "replacement_tc.mp4"
```

实际编码参数需要和分发码流兼容：编码族、分辨率、profile、level、像素格式、
extradata、timebase，以及预期帧率行为都要一致。

## 导出模型

导出永远以主轨道顺序为准。

MP4 导出：

- 从每个 GOP 片段复制压缩视频包。
- 所有替换 GOP 的视频流参数必须兼容。
- 可以复制与主轨道 GOP 时间范围重叠的音频包。
- 如果所有来源一致，可以保留与来源兼容的 timebase；也可以手动设置 timebase。
- 可以为 GOP 重排输出生成连续 PTS/DTS。

裸流导出：

- 写出纯视频 H.264 或 HEVC elementary stream。
- 使用 FFmpeg bitstream filter 输出 Annex B。
- 不携带音频、字幕、容器 metadata、PTS 或 DTS。

## 兼容性

当前范围是 H.264 和 HEVC 分发码流。

为了可靠替换，替换 GOP 应该匹配主分发码流：

- 同一编码族：H.264 对 H.264，HEVC 对 HEVC。
- 相同分辨率、profile、level、像素格式和 codec extradata。
- MP4 输出时，timebase 和预期帧率行为要兼容。
- GOP 切点必须从索引器识别到的关键包开始。
- 替换片段应该覆盖原始 GOP 的同一显示帧范围。

ProRes 这类不在范围内的编辑目标会被有意拒绝。GOP Composer 不是通用 NLE，也不做
任意帧级剪辑。

## 从源码构建

这个 Rust 项目始终使用 release 构建：

```powershell
cargo build --release
```

运行：

```powershell
.\target\release\gop-composer.exe
```

### FFmpeg

GOP Composer 链接 FFmpeg shared libraries。Windows 上构建脚本会优先查找项目内的
自定义 FFmpeg：

```text
vendor\ffmpeg-gopcomposer-win64-msvc
```

构建命令：

```powershell
.\scripts\build-ffmpeg-windows-msvc.ps1 -Clean
```

也可以把 `FFMPEG_DIR` 指向已有 shared FFmpeg：

```powershell
$env:FFMPEG_DIR = "D:\path\to\ffmpeg-shared"
```

Windows 下该目录必须包含 `include`、`lib` 和 `bin`。运行时 DLL 会从 `bin` 复制到
`target\release`。

自定义 Windows FFmpeg 构建刻意保持较小范围：

- Libraries: `avcodec`, `avformat`, `avutil`, `swscale`
- Demuxers: Matroska/MKV, MOV/MP4, MPEG-TS, raw H.264, raw HEVC
- Muxers: MP4/MOV
- Decoders: H.264 and HEVC
- Hardware decode: H.264/HEVC through D3D11VA, D3D12VA, and DXVA2
- Bitstream filters: `h264_mp4toannexb`, `hevc_mp4toannexb`, `aac_adtstoasc`,
  `extract_extradata`

`swresample` 不需要，因为 GOP Composer 只复制音频包，不解码、不重采样、不编码音频。
`swscale` 仍然需要，因为 CPU RGBA fallback 预览路径会用到。

## CLI 工具

GUI 是主要入口，但项目也提供一些用于检查和 smoke test 的 CLI 命令：

```powershell
.\target\release\gop-composer.exe --probe "input.mkv"
.\target\release\gop-composer.exe --decode-window "input.mkv" 250 16
.\target\release\gop-composer.exe --export-gops "input.mkv" "2,5" mp4 "output.mp4"
.\target\release\gop-composer.exe --export-gops "input.mkv" "2-3" raw "output.h264"
.\target\release\gop-composer.exe --export-gops-ordered "input.mkv" "2,1,2" mp4 "output.mp4" --generate-pts
.\target\release\gop-composer.exe --timecode-map "main.mp4" "replacement_tc.mp4"
.\scripts\smoke-release.ps1 -MkvSample "sample.mkv" -HevcSample "sample-hevc.mp4"
```

## 快捷键

| 快捷键 | 动作 |
| --- | --- |
| Left / Right | 按一帧步进预览 |
| Up / Down | 在当前时间线轨道上切换 GOP |
| Delete | 删除已选择的时间线 GOP |
| Ctrl+X / Ctrl+C / Ctrl+V | 剪切、复制、粘贴所选 GOP |
| Ctrl+Z / Ctrl+Y | 撤销和重做时间线编辑 |
| Space | 播放或暂停预览 |

## 项目结构

```text
src/app/       应用状态和编辑工作流
src/media/     FFmpeg 相关媒体操作
src/preview/   预览 worker 和 GPU 上传路径
src/ui/        egui 面板、样式、i18n 和自绘窗口栏
native/        FFmpeg 和预览互操作的 C bridge
scripts/       构建和 smoke test 脚本
docs/          开源准备记录
```

## 平台状态

Windows 是目前开发和测试最完整的路径。项目结构已经按平台相关预览和窗口集成拆分，
后续可以通过条件编译扩展 macOS 或 Linux。

## 限制

- 编辑基于 GOP 边界，不是任意帧边界。
- GOP Composer 不会替你转码替换片段。
- MP4 导出目前面向主视频和可选音频。
- 还没有实现字幕流混合。
- 替换文件音频处理保持保守。主要用例是针对兼容分发码流做视频 GOP 替换。
- 生产使用前，仍然应该用播放器、FFmpeg/ffprobe 和交付 QC 工具验证导出文件。

## 许可证

GOP Composer 使用 [MIT License](LICENSE) 发布。
