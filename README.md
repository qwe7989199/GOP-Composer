# GOP Composer

Languages: [English](#english) | [中文](#中文)

## English

GOP Composer is a local desktop tool for GOP-level replacement and remuxing of
distribution video streams. It is built for cases where a finished H.264/HEVC
delivery file needs a small visual fix, but re-encoding the whole asset would be
slow, lossy, or operationally wasteful.

The core idea is simple: keep every unchanged GOP as compressed packets, replace
only the GOPs that need new pixels, and mux the final main track back to MP4 or
raw H.264/HEVC without re-encoding unchanged parts.

The application UI supports English and Chinese. Use the `中文` / `English`
button in the top toolbar to switch language instantly.

### Why this exists

Video production workflows often discover tiny mistakes after a full delivery
encode: a burned-in subtitle typo, one bad shot, one incorrect graphic, or a
small source replacement. Professional mastering workflows can solve similar
problems with incremental package formats such as IMF, but streaming
distribution files are usually delivered as ordinary MP4/MKV/MOV assets. In
practice, a one-line subtitle fix often means re-encoding the entire movie.

GOP Composer targets that gap. If the affected line belongs to one GOP, or a
small continuous group of GOPs, you can:

1. Locate the affected GOPs in the distribution stream.
2. Re-encode only the matching frame range with compatible codec settings.
3. Import both the original delivery file and the replacement source.
4. Drag the replacement GOPs into the main track, or apply them by matching
   timecode.
5. Export a new MP4 or elementary stream by copying compressed packets.

For this use case, unchanged GOPs remain bit-identical compressed video.

### Timecode-assisted replacement

When you use `Add videos`, GOP Composer checks the added source for readable
timecode. If the replacement source and the main track have matching timecode
rates, the app compares GOP start frames by absolute timecode frame:

```text
absolute frame = timecode start frame + GOP start frame
```

If replacement GOPs match main-track GOP starts, the app asks whether to apply
the replacement immediately. Click `Apply` to replace the matching GOPs on the
main track. Click `Cancel` to keep the source imported without changing the main
track.

GOP Composer only performs the mechanical mapping. You are responsible for
ensuring the replacement frame range, GOP boundaries, and visual content are
actually correct.

#### Add timecode with FFmpeg

If the replacement file is already encoded correctly and only needs timecode
metadata, remux it:

```powershell
ffmpeg -y -i "replacement_no_tc.mp4" -map 0 -c copy `
  -timecode "01:00:04:00" `
  -metadata:s:v:0 timecode="01:00:04:00" `
  "replacement_tc.mp4"
```

If you are encoding the replacement section, write the timecode during encode.
Set the timecode to the main-track GOP start that the first replacement GOP
should replace:

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

For HEVC, use an equivalent x265 GOP configuration:

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

The exact codec parameters must remain compatible with the delivery stream:
codec family, resolution, profile, level, pixel format, extradata, timebase, and
intended frame-rate behavior.

#### GUI workflow

1. Click `Open Main Video` and import the delivery file.
2. Click `Add videos` and import the replacement file with timecode.
3. If a match is found, confirm `Apply timecode replacement?`.
4. Inspect the main track and preview.
5. Export MP4 or raw H.264/HEVC.

#### CLI mapping check

This command only reports the mapping. It does not modify files:

```powershell
.\target\release\gop-composer.exe --timecode-map "main.mp4" "replacement_tc.mp4"
```

### Current features

- Local-first desktop GUI built with Rust, egui, and eframe.
- One-click English/Chinese UI switching.
- GOP indexing for common FFmpeg-supported containers.
- H.264 and HEVC focus for preview, compatibility checks, raw export, and MP4
  remuxing.
- Hardware-accelerated preview on Windows through FFmpeg D3D11VA/D3D12VA/DXVA2
  when available.
- Multi-source timeline with GOP selection, copy, cut, paste, delete, undo,
  redo, and cross-track drag insertion.
- Main-track export by GOP order.
- MP4 export with timestamp rewriting, optional generated PTS, configurable
  timebase, and optional audio muxing from the main track time ranges.
- Raw H.264/HEVC Annex B export for video-only workflows.
- GOP list, source metadata, stream metadata, and compatibility inspection.
- Timecode probing and replacement-to-main GOP application.
- All media processing is local. Files are not uploaded.

### Export model

Exports always use the main track as the ordered source of truth.

MP4 export:

- Copies compressed video packets from each GOP segment.
- Requires compatible video stream parameters across replacement GOPs.
- Can include audio packets whose source timestamps overlap each selected GOP
  time range.
- Can keep source-relative packet timestamps or generate CFR-style PTS/DTS.
- Supports a source timebase when all main-track sources match, or a custom
  timebase when they do not.

Raw export:

- Writes video-only H.264 or HEVC elementary streams.
- Uses FFmpeg bitstream filters to output Annex B.
- Does not carry audio, subtitles, container metadata, PTS, or DTS.

### Compatibility rules

For reliable replacement, source GOPs should match the delivery stream:

- Same video codec family: H.264 with H.264, HEVC with HEVC.
- Same resolution, profile, level, pixel format, and codec extradata.
- Same or intentionally controlled timebase and frame-rate behavior for MP4
  output.
- GOP cuts must start on key packets detected by the indexer.
- Replacement clips should cover the same display frame range as the original
  GOPs they replace.

The app intentionally rejects unsupported codecs such as ProRes for GOP
replacement. The current scope is H.264/HEVC distribution streams.

### Build requirements

GOP Composer links against shared FFmpeg libraries. On Windows, the build script
first looks for the project-local custom FFmpeg build:

```text
vendor\ffmpeg-gopcomposer-win64-msvc
```

Build it with:

```powershell
.\scripts\build-ffmpeg-windows-msvc.ps1 -Clean
```

Alternatively, point `FFMPEG_DIR` to a shared FFmpeg install:

```powershell
$env:FFMPEG_DIR = "D:\path\to\ffmpeg-shared"
```

The directory must contain `include`, `lib`, and `bin` on Windows. Runtime DLLs
are copied from `bin` into `target\release`.

The custom Windows FFmpeg build is intentionally narrow:

- Libraries: `avcodec`, `avformat`, `avutil`, `swscale`
- Demux: Matroska/MKV, MOV/MP4, MPEG-TS, raw H.264, raw HEVC
- Mux: MP4/MOV
- Decode: H.264 and HEVC
- Hardware decode: H.264/HEVC through D3D11VA, D3D12VA, and DXVA2
- Bitstream filters: `h264_mp4toannexb`, `hevc_mp4toannexb`, `aac_adtstoasc`,
  `extract_extradata`

`swresample` is deliberately not built because GOP Composer copies audio
packets and does not decode, resample, or encode audio. `swscale` is still
required by the CPU RGBA fallback preview path.

### Build

Always use release builds for this Rust project:

```powershell
cargo build --release
```

### Run

```powershell
.\target\release\gop-composer.exe
```

### Useful CLI checks

```powershell
.\target\release\gop-composer.exe --probe "input.mkv"
.\target\release\gop-composer.exe --decode-window "input.mkv" 250 16
.\target\release\gop-composer.exe --export-gops "input.mkv" "2,5" mp4 "output.mp4"
.\target\release\gop-composer.exe --export-gops "input.mkv" "2-3" raw "output.h264"
.\target\release\gop-composer.exe --export-gops-ordered "input.mkv" "2,1,2" mp4 "output.mp4"
.\target\release\gop-composer.exe --timecode-map "main.mp4" "replacement_tc.mp4"
.\scripts\smoke-release.ps1 -MkvSample "sample.mkv" -HevcSample "sample-hevc.mp4"
```

### Keyboard shortcuts

- Left/Right: step preview by one frame.
- Up/Down: move between GOPs on the current timeline track.
- Delete: delete selected timeline GOPs.
- Ctrl+X/Ctrl+C/Ctrl+V: cut, copy, and paste selected GOPs.
- Ctrl+Z/Ctrl+Y: undo and redo timeline edits.
- Space: play or pause preview playback.

### Current limits

- H.264 and HEVC are the intended video codecs.
- Editing is GOP-boundary based, not arbitrary frame-boundary based.
- MP4 export currently muxes primary video and optional audio only.
- Subtitle stream muxing is not implemented yet.
- Audio from multiple replacement files is intentionally conservative; the main
  use case is video GOP replacement against a compatible delivery stream.
- Cross-platform work is planned, but Windows is the most developed path today.

### Project status

This project is in early development. It is usable for controlled GOP
replacement experiments, but exported files should be validated with your own
player, FFmpeg/ffprobe, and delivery QC tooling before production use.

### License

No open-source license has been selected yet.

## 中文

GOP Composer 是一个本地桌面工具，用于在分发视频码流上进行 GOP 级替换和
重新封装。它面向这样的场景：一个已经完成交付编码的 H.264/HEVC 文件只需要
修正很小一段画面，但重新编码整条素材既慢、又有损、也不划算。

核心思路很直接：未变化的 GOP 继续保留原始压缩包，只替换需要新画面的 GOP，
最后把主轨道重新封装为 MP4 或 H.264/HEVC 裸流。未变化的部分不重新编码。

程序界面支持英文和中文。点击顶部工具栏里的 `中文` / `English` 按钮即可即时
切换语言。

### 项目初衷

视频制作流程经常会在完整输出后才发现小错误：烧录硬字幕的一句错译、一个坏镜头、
一张错误图卡，或者一小段需要换源。专业母版流程可以用 IMF 这类增量包解决类似
问题，但流媒体分发端通常还是普通 MP4/MKV/MOV 资产。实际工作中，一句字幕错误
往往意味着整部影片重新编码。

GOP Composer 解决的就是这个空档。如果受影响内容只落在一个 GOP，或者连续几个
GOP 内，可以这样处理：

1. 在分发码流中定位受影响 GOP。
2. 只用兼容编码参数重新压制对应帧范围。
3. 导入原始分发文件和替换素材。
4. 手动把替换 GOP 拖到主轨道，或者按时间码自动应用。
5. 复制压缩包并重新封装，导出新的 MP4 或裸流。

在这个使用模型下，未修改 GOP 仍然是 bit-identical 的压缩视频数据。

### 时间码辅助替换

执行 `Add videos` 时，GOP Composer 会检查新增来源是否有可读取的时间码。如果
替换来源和主轨道的时间码帧率一致，程序会用绝对时间码帧匹配 GOP 起点：

```text
绝对帧 = 时间码起始帧 + GOP 起始帧
```

如果替换来源的 GOP 能匹配主轨道 GOP 起点，程序会询问是否立即应用。点击
`应用` (`Apply`) 会把匹配到的 GOP 直接替换到主轨道对应位置；点击 `取消`
(`Cancel`) 只保留导入来源，不修改主轨道。

GOP Composer 只负责机械映射。替换片段的帧范围、GOP 边界和画面内容是否真的
对应，由用户自己保证。

#### 用 FFmpeg 写入时间码

如果替换文件已经按正确参数编码好了，只需要追加时间码 metadata，可以直接重新封装：

```powershell
ffmpeg -y -i "replacement_no_tc.mp4" -map 0 -c copy `
  -timecode "01:00:04:00" `
  -metadata:s:v:0 timecode="01:00:04:00" `
  "replacement_tc.mp4"
```

如果替换片段是在这一步重新压制，可以在编码时写入时间码。这里的时间码应该设置为
替换片段第一个 GOP 对应的主轨道 GOP 起始时间码：

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

HEVC 可以使用等价的 x265 GOP 参数：

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

实际编码参数必须和分发码流兼容：编码族、分辨率、profile、level、像素格式、
extradata、时间基，以及预期的帧率行为都要一致。

#### GUI 流程

1. 点击 `打开主视频` (`Open Main Video`) 导入分发文件。
2. 点击 `添加视频` (`Add videos`) 导入带时间码的替换文件。
3. 如果发现匹配，确认 `应用时间码替换？` (`Apply timecode replacement?`)。
4. 检查主轨道和预览画面。
5. 导出 MP4 或 H.264/HEVC 裸流。

#### CLI 映射检查

这个命令只打印映射关系，不修改文件：

```powershell
.\target\release\gop-composer.exe --timecode-map "main.mp4" "replacement_tc.mp4"
```

### 当前功能

- 使用 Rust、egui、eframe 构建的本地优先桌面 GUI。
- 一键切换中文/英文界面。
- 对常见 FFmpeg 容器建立 GOP 索引。
- 围绕 H.264 和 HEVC 做预览、兼容检查、裸流导出和 MP4 重新封装。
- Windows 下可用 FFmpeg D3D11VA/D3D12VA/DXVA2 硬件加速预览。
- 多来源时间线，支持 GOP 选择、复制、剪切、粘贴、删除、撤销、重做和跨轨道拖入。
- 永远按主轨道 GOP 顺序导出。
- MP4 导出支持时间戳重写、可选生成 PTS、可配置 timebase，以及按主轨道时间范围混合音频。
- H.264/HEVC Annex B 裸流导出。
- GOP 列表、来源 metadata、流 metadata 和兼容性检查。
- 时间码探测，以及按时间码把替换 GOP 应用到主轨道。
- 所有媒体处理都在本地完成，文件不会上传。

### 导出模型

导出永远以主轨道顺序为准。

MP4 导出：

- 从每个 GOP 片段复制压缩视频包。
- 替换 GOP 之间需要兼容的视频流参数。
- 可包含与所选 GOP 时间范围重叠的音频包。
- 可保留来源相对时间戳，也可生成 CFR 风格 PTS/DTS。
- 如果所有主轨道来源 timebase 一致，可以使用源 timebase；否则可以手动设置。

裸流导出：

- 写出纯视频 H.264 或 HEVC elementary stream。
- 使用 FFmpeg bitstream filter 输出 Annex B。
- 不包含音频、字幕、容器 metadata、PTS 或 DTS。

### 兼容规则

为了可靠替换，来源 GOP 应该匹配分发码流：

- 同一编码族：H.264 替换 H.264，HEVC 替换 HEVC。
- 相同分辨率、profile、level、像素格式和 codec extradata。
- MP4 输出时，timebase 和帧率行为需要一致或被有意控制。
- GOP 切点必须从索引器识别到的关键包开始。
- 替换片段应该覆盖原始 GOP 的相同显示帧范围。

程序会有意拒绝 ProRes 等不在范围内的编码。当前目标就是 H.264/HEVC 分发码流。

### 构建要求

GOP Composer 链接 FFmpeg shared libraries。Windows 上构建脚本会优先查找项目内
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

该目录在 Windows 下必须包含 `include`、`lib` 和 `bin`。运行时 DLL 会从 `bin`
复制到 `target\release`。

自定义 Windows FFmpeg 构建刻意保持较小范围：

- Libraries: `avcodec`, `avformat`, `avutil`, `swscale`
- Demux: Matroska/MKV, MOV/MP4, MPEG-TS, raw H.264, raw HEVC
- Mux: MP4/MOV
- Decode: H.264 and HEVC
- Hardware decode: H.264/HEVC through D3D11VA, D3D12VA, and DXVA2
- Bitstream filters: `h264_mp4toannexb`, `hevc_mp4toannexb`, `aac_adtstoasc`,
  `extract_extradata`

`swresample` 没有构建，因为 GOP Composer 只复制音频包，不解码、不重采样、不编码音频。
`swscale` 仍然需要，因为 CPU RGBA fallback 预览路径会用到。

### 构建

这个 Rust 项目始终使用 release 构建：

```powershell
cargo build --release
```

### 运行

```powershell
.\target\release\gop-composer.exe
```

### 常用 CLI 检查

```powershell
.\target\release\gop-composer.exe --probe "input.mkv"
.\target\release\gop-composer.exe --decode-window "input.mkv" 250 16
.\target\release\gop-composer.exe --export-gops "input.mkv" "2,5" mp4 "output.mp4"
.\target\release\gop-composer.exe --export-gops "input.mkv" "2-3" raw "output.h264"
.\target\release\gop-composer.exe --export-gops-ordered "input.mkv" "2,1,2" mp4 "output.mp4"
.\target\release\gop-composer.exe --timecode-map "main.mp4" "replacement_tc.mp4"
.\scripts\smoke-release.ps1 -MkvSample "sample.mkv" -HevcSample "sample-hevc.mp4"
```

### 快捷键

- 左/右：按一帧步进预览。
- 上/下：在当前时间线轨道上切换 GOP。
- Delete：删除已选择的时间线 GOP。
- Ctrl+X/Ctrl+C/Ctrl+V：剪切、复制、粘贴所选 GOP。
- Ctrl+Z/Ctrl+Y：撤销和重做时间线编辑。
- Space：播放或暂停预览。

### 当前限制

- 目标视频编码是 H.264 和 HEVC。
- 编辑基于 GOP 边界，不是任意帧边界。
- MP4 导出当前只处理主视频和可选音频。
- 还没有实现字幕流混合。
- 多替换文件音频仍然保守处理；主要目标是针对兼容分发码流做视频 GOP 替换。
- 跨平台会继续推进，但目前 Windows 路径最完整。

### 项目状态

项目仍处于早期开发阶段。它已经可以用于受控的 GOP 替换实验，但生产使用前仍然应该
用播放器、FFmpeg/ffprobe 和交付 QC 工具自行验证导出文件。

### License

还没有选择开源许可证。
