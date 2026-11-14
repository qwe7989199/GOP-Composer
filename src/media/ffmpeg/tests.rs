use std::{fs, path::PathBuf};

use super::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn segment(name: &str, gop: i64) -> ExportSegment {
    ExportSegment {
        input_path: fixture(name),
        range: PacketRange {
            start_packet: gop * 12,
            end_packet: gop * 12 + 11,
        },
    }
}

#[test]
fn synthetic_fixtures_have_expected_gops_and_timecode() {
    for (name, codec, count) in [
        ("h264-main.mp4", "h264", 6),
        ("h264-audio.mkv", "h264", 6),
        ("h264-replacement.mp4", "h264", 2),
        ("hevc-main10.mp4", "hevc", 4),
    ] {
        let probe = probe_gops(&fixture(name)).unwrap();
        assert_eq!(probe.codec_name, codec);
        assert_eq!((probe.width, probe.height), (160, 96));
        assert_eq!(probe.frame_count, count * 12);
        assert_eq!(probe.gops.len(), count as usize);
        for (i, gop) in probe.gops.iter().enumerate() {
            assert_eq!((gop.start_frame, gop.frames()), (i as i64 * 12, 12));
        }
    }
    let main = probe_gops(&fixture("h264-main.mp4"))
        .unwrap()
        .timecode
        .unwrap();
    let replacement = probe_gops(&fixture("h264-replacement.mp4"))
        .unwrap()
        .timecode
        .unwrap();
    assert_eq!((main.rate_num, main.rate_den), (24, 1));
    assert_eq!(main.start_frame, 24 * 3600);
    assert_eq!(replacement.start_frame - main.start_frame, 24);
}

#[test]
fn preview_decodes_across_gop_boundaries() {
    for name in ["h264-main.mp4", "hevc-main10.mp4"] {
        let input = fixture(name);
        let probe = probe_gops(&input).unwrap();
        let frames = decode_rgba_window(&input, 0, probe.gops[0].pts, 10, 8).unwrap();
        assert_eq!(frames.len(), 8);
        for (offset, frame) in frames.iter().enumerate() {
            assert_eq!(frame.frame_index, 10 + offset as i64);
            assert_eq!(frame.frame.pixels.len(), 160 * 96 * 4);
        }
    }
}

#[test]
fn ordered_b_frame_gops_have_no_timestamp_gaps() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["h264-main.mp4", "hevc-main10.mp4"] {
        for generate_pts in [false, true] {
            let output = temp.path().join("ordered.mp4");
            let segments = [segment(name, 1), segment(name, 0), segment(name, 1)];
            export_video_segments(
                &segments,
                &output,
                ExportKind::Mp4,
                ExportOptions {
                    generate_pts,
                    ..Default::default()
                },
            )
            .unwrap();
            let probe = probe_gops(&output).unwrap();
            assert_eq!(probe.frame_count, 36);
            assert_eq!(probe.gops.len(), 3);
            for pair in probe.gops.windows(2) {
                assert!(
                    (pair[1].start_time - pair[0].start_time - 0.5).abs() < 0.001,
                    "{name}, generate_pts={generate_pts}: {pair:?}"
                );
            }
        }
    }
}

#[test]
fn compatible_replacement_exports_the_entire_main_track() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("replaced.mp4");
    let segments = [
        segment("h264-main.mp4", 0),
        segment("h264-main.mp4", 1),
        segment("h264-replacement.mp4", 0),
        segment("h264-replacement.mp4", 1),
        segment("h264-main.mp4", 4),
        segment("h264-main.mp4", 5),
    ];
    export_video_segments(
        &segments,
        &output,
        ExportKind::Mp4,
        ExportOptions::default(),
    )
    .unwrap();
    let probe = probe_gops(&output).unwrap();
    assert_eq!(probe.frame_count, 72);
    assert_eq!(probe.gops.len(), 6);
    let expected = decode_rgba_frame(&fixture("h264-replacement.mp4"), 0).unwrap();
    let actual = decode_rgba_frame(&output, 24).unwrap();
    assert_eq!(actual.pixels, expected.pixels);
}

#[test]
fn exports_reject_source_paths_and_hardlink_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("源视频.mp4");
    let alias = temp.path().join("alias.mp4");
    fs::copy(fixture("h264-main.mp4"), &input).unwrap();
    fs::hard_link(&input, &alias).unwrap();
    let original = fs::read(&input).unwrap();
    for output in [&input, &alias] {
        for kind in [ExportKind::Mp4, ExportKind::Raw] {
            let error = export_video_range(&input, output, 0, 11, kind).unwrap_err();
            assert!(error.contains("must not overwrite"), "{error}");
            let segments = [ExportSegment {
                input_path: input.clone(),
                range: PacketRange {
                    start_packet: 0,
                    end_packet: 11,
                },
            }];
            assert!(
                export_video_segments(&segments, output, kind, ExportOptions::default()).is_err()
            );
            assert_eq!(fs::read(&input).unwrap(), original);
        }
    }
}

#[test]
fn failed_exports_preserve_destination_and_clean_temporary_files() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("keep.mp4");
    fs::write(&output, b"previous export").unwrap();
    // Failure occurs after the first segment was written, not just at validation.
    let segments = [segment("h264-main.mp4", 0), segment("hevc-main10.mp4", 0)];
    for kind in [ExportKind::Mp4, ExportKind::Raw] {
        assert!(export_video_segments(&segments, &output, kind, ExportOptions::default()).is_err());
        assert_eq!(fs::read(&output).unwrap(), b"previous export");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
    }
    assert!(
        export_video_range(&fixture("h264-main.mp4"), &output, 12, 0, ExportKind::Mp4).is_err()
    );
    assert_eq!(fs::read(&output).unwrap(), b"previous export");
}

#[test]
fn raw_exports_round_trip_with_unicode_paths() {
    let temp = tempfile::tempdir().unwrap();
    for (name, extension) in [("h264-main.mp4", "h264"), ("hevc-main10.mp4", "h265")] {
        let input = temp.path().join("测试 输入.mp4");
        fs::copy(fixture(name), &input).unwrap();
        let output = temp.path().join(format!("测试 输出.{extension}"));
        let result = export_video_range(&input, &output, 12, 23, ExportKind::Raw).unwrap();
        assert_eq!(result.packets_written, 12);
        assert_eq!(
            result.bytes_written as u64,
            fs::metadata(&output).unwrap().len()
        );
        assert_eq!(probe_gops(&output).unwrap().frame_count, 12);
        assert_eq!(
            decode_rgba_frame(&output, 0).unwrap().pixels,
            decode_rgba_frame(&input, 12).unwrap().pixels
        );
    }
}
