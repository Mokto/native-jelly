use super::*;

const NATIVE: c_int = ORDER_NATIVE;

#[test]
fn mono_and_stereo_pass_straight_through() {
    let mono = plan_mix(NATIVE, 1, FC).unwrap();
    assert_eq!((mono.out_channels, mono.out_name), (1, "mono"));
    let stereo = plan_mix(NATIVE, 2, FL | FR).unwrap();
    assert_eq!((stereo.out_channels, stereo.out_name), (2, "stereo"));
    assert_eq!(stereo.routes, vec![vec![(0, 1.0)], vec![(1, 1.0)]]);
}

#[test]
fn five_one_maps_one_to_one_whichever_surround_pair_it_names() {
    let side = plan_mix(NATIVE, 6, OUT_5_1).unwrap();
    assert_eq!(side.out_name, "5.1");
    assert_eq!(side.routes, (0..6).map(|o| vec![(o, 1.0)]).collect::<Vec<_>>());
    // 5.1(back) in native order: FL FR FC LFE BL BR — the backs become the sides.
    let back = plan_mix(NATIVE, 6, FL | FR | FC | LFE | BL | BR).unwrap();
    assert_eq!(back.routes, (0..6).map(|o| vec![(o, 1.0)]).collect::<Vec<_>>());
}

#[test]
fn seven_one_folds_each_back_channel_into_its_side_at_minus_3_db() {
    // Native order: FL FR FC LFE BL BR SL SR.
    let m = plan_mix(NATIVE, 8, OUT_5_1 | BL | BR).unwrap();
    let h = std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(m.out_channels, 6);
    assert_eq!(m.routes, vec![
        vec![(0, 1.0)], vec![(1, 1.0)], vec![(2, 1.0)], vec![(3, 1.0)],
        vec![(4, h)], vec![(5, h)], vec![(4, h)], vec![(5, h)],
    ]);
}

#[test]
fn an_unspecified_order_takes_ffmpegs_default_for_the_count() {
    assert_eq!(plan_mix(ORDER_UNSPEC, 2, 0).unwrap().out_name, "stereo");
    assert_eq!(plan_mix(ORDER_UNSPEC, 8, 0).unwrap().routes.len(), 8);
    assert!(plan_mix(ORDER_UNSPEC, 5, 0).is_none(), "no default this module trusts");
}

#[test]
fn layouts_without_an_honest_ac3_mix_are_refused() {
    const TOP_CENTER: u64 = 1 << 11;
    assert!(plan_mix(NATIVE, 7, OUT_5_1 | TOP_CENTER).is_none(), "a height channel would be lost");
    assert!(plan_mix(NATIVE, 3, FL | FR).is_none(), "mask and count disagree");
    assert!(plan_mix(2, 2, 0).is_none(), "custom channel maps");
    assert!(plan_mix(NATIVE, 10, OUT_5_1 | BL | BR | FLC | FRC).is_none(), "past 7.1");
}

#[test]
fn three_point_oh_and_quad_become_5_1_with_silent_gaps() {
    let surround = plan_mix(NATIVE, 3, FL | FR | FC).unwrap();
    assert_eq!(surround.routes, vec![vec![(0, 1.0)], vec![(1, 1.0)], vec![(2, 1.0)]]);
    let h = std::f32::consts::FRAC_1_SQRT_2;
    let four = plan_mix(NATIVE, 4, FL | FR | FC | BC).unwrap();
    assert_eq!(four.routes[3], vec![(4, h), (5, h)], "the back centre feeds both sides");
}

fn bytes<T: Copy, const N: usize>(values: &[T], to: impl Fn(T) -> [u8; N]) -> Vec<u8> {
    values.iter().flat_map(|v| to(*v)).collect()
}

#[test]
fn interleaved_integer_samples_are_scaled_and_deinterleaved() {
    let mix = plan_mix(NATIVE, 2, FL | FR).unwrap();
    let s16 = bytes(&[16_384i16, -32_768, 0, 32_767], i16::to_ne_bytes);
    let mut out = vec![Vec::new(), Vec::new()];
    assert!(mix_into(fmt::S16, &[&s16], 2, 2, &mix, &mut out));
    assert_eq!(out[0], vec![0.5, 0.0]);
    assert_eq!(out[1][0], -1.0);
    assert!((out[1][1] - 1.0).abs() < 1e-4);
    // TrueHD's own output: 24-bit audio in the top of an s32.
    let s32 = bytes(&[i32::MIN, 1 << 30], i32::to_ne_bytes);
    let mut out = vec![Vec::new(), Vec::new()];
    assert!(mix_into(fmt::S32, &[&s32], 2, 1, &mix, &mut out));
    assert_eq!((out[0][0], out[1][0]), (-1.0, 0.5));
}

#[test]
fn planar_float_and_u8_read_one_plane_per_channel() {
    let mix = plan_mix(NATIVE, 2, FL | FR).unwrap();
    let left = bytes(&[0.25f32, -0.25], f32::to_ne_bytes);
    let right = bytes(&[0.75f32, 2.0], f32::to_ne_bytes);
    let mut out = vec![Vec::new(), Vec::new()];
    assert!(mix_into(fmt::FLTP, &[&left, &right], 2, 2, &mix, &mut out));
    assert_eq!(out, vec![vec![0.25, -0.25], vec![0.75, 1.0]], "out of range clamps");
    let u8s = [128u8, 0];
    let mut out = vec![Vec::new()];
    let mono = plan_mix(NATIVE, 1, FC).unwrap();
    assert!(mix_into(fmt::U8P, &[&u8s], 1, 2, &mono, &mut out));
    assert_eq!(out[0], vec![0.0, -1.0]);
}

#[test]
fn a_fold_sums_into_the_side_channel_and_appends_after_what_is_queued() {
    let m = plan_mix(NATIVE, 8, OUT_5_1 | BL | BR).unwrap();
    // One sample: every channel at 0.5.
    let frame = bytes(&[0.5f32; 8], f32::to_ne_bytes);
    let mut out = vec![vec![9.0]; 6];
    assert!(mix_into(fmt::FLT, &[&frame], 8, 1, &m, &mut out));
    assert_eq!(out[0], vec![9.0, 0.5], "appended, the queued sample untouched");
    let side = out[4][1];
    assert!((side - 0.5 * 2.0 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
}

#[test]
fn short_planes_and_unknown_formats_are_refused() {
    let mix = plan_mix(NATIVE, 2, FL | FR).unwrap();
    let mut out = vec![Vec::new(), Vec::new()];
    assert!(!mix_into(fmt::S16, &[&[0u8; 6]], 2, 2, &mix, &mut out));
    assert!(!mix_into(99, &[&[0u8; 64]], 2, 2, &mix, &mut out));
    assert!(!mix_into(fmt::S16P, &[&[0u8; 4]], 2, 2, &mix, &mut out), "one plane for two channels");
    assert!(out.iter().all(Vec::is_empty), "nothing appended on refusal");
}

fn fill(f: &mut Fifo, n: usize) {
    for c in &mut f.chans {
        c.extend(std::iter::repeat(0.0).take(n));
    }
}

#[test]
fn frames_are_stamped_by_sample_count_from_the_first_timestamp() {
    let mut f = Fifo::new(2, 48_000);
    assert!(!f.anchor(None), "nothing to count from yet");
    assert!(f.anchor(Some(10_000_000_000)));
    fill(&mut f, 1536 * 2 + 10);
    assert_eq!(f.pop(1536).unwrap().0, 10_000_000_000);
    assert_eq!(f.pop(1536).unwrap().0, 10_032_000_000, "1536 samples at 48 kHz are 32 ms");
    assert!(f.pop(1536).is_none(), "10 samples are not a frame");
    assert_eq!(f.chans[0].len(), 10);
}

#[test]
fn a_timestamp_close_to_the_count_keeps_it_and_a_far_one_reanchors() {
    let mut f = Fifo::new(1, 48_000);
    f.anchor(Some(0));
    fill(&mut f, 4800); // 100 ms queued
    assert!(f.anchor(Some(100_000_000 + REANCHOR_NS / 2)), "jitter keeps the count");
    assert_eq!(f.chans[0].len(), 4800);
    assert!(f.anchor(Some(5_000_000_000)));
    assert!(f.chans[0].is_empty(), "a gap drops the partial frame");
    fill(&mut f, 1536);
    assert_eq!(f.pop(1536).unwrap().0, 5_000_000_000);
}

#[test]
fn a_reset_waits_for_the_next_timestamp() {
    let mut f = Fifo::new(1, 44_100);
    f.anchor(Some(1));
    fill(&mut f, 100);
    f.reset();
    assert!(f.chans[0].is_empty());
    assert!(!f.anchor(None));
    assert!(f.anchor(Some(7_000_000_000)));
    fill(&mut f, 1536);
    assert_eq!(f.pop(1536).unwrap().0, 7_000_000_000);
}

#[test]
fn bitrates_follow_the_output_width() {
    assert_eq!((ac3_bitrate(1), ac3_bitrate(2), ac3_bitrate(6)), (192_000, 256_000, 640_000));
}

#[test]
fn only_the_converted_formats_are_converted() {
    for codec in ["truehd", "TrueHD", "flac", "pcm_bluray", "vorbis"] {
        assert!(wants_conversion(codec), "{codec}");
    }
    for codec in ["ac3", "eac3", "aac", "dts", "dca", "opus", ""] {
        assert!(!wants_conversion(codec), "{codec}");
    }
}

// ---- through the real bundled FFmpeg (`make check-ffmpeg`) ------------------------------------

/// Write `secs` of pink noise in `codec` with `layout` at `rate` into `dir/name`; `None` when the
/// `ffmpeg` command line tool is absent (a skip, except under CI).
fn fixture(dir: &std::path::Path, name: &str, codec: &str, layout: &str, rate: u32, secs: u32) -> Option<std::path::PathBuf> {
    let path = dir.join(name);
    let status = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("anoisesrc=d={secs}:c=pink:r={rate}:a=0.3"))
        .args(["-af", &format!("aformat=channel_layouts={layout}"), "-c:a", codec, "-strict", "experimental"])
        .arg(&path)
        .status();
    match status {
        Ok(s) if s.success() => Some(path),
        Ok(s) => panic!("ffmpeg could not write the {name} fixture: {s}"),
        Err(_) if std::env::var_os("CI").is_some() => panic!("ffmpeg is required under CI"),
        Err(_) => {
            eprintln!("SKIP: no ffmpeg command line tool to write fixtures with");
            None
        }
    }
}

/// Open `path`'s only audio stream and run every packet through a [`Converter`].
unsafe fn convert_file(path: &std::path::Path) -> Result<(String, Vec<(i64, Vec<u8>)>), String> {
    let url = CString::new(path.to_str().unwrap()).unwrap();
    let mut fmt = std::ptr::null_mut();
    assert!(avformat_open_input(&mut fmt, url.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut()) >= 0);
    assert!(avformat_find_stream_info(fmt, std::ptr::null_mut()) >= 0);
    let st = *(*fmt).streams;
    let result = Converter::open(stream_codecpar(st)).map(|mut conv| {
        let pkt = av_packet_alloc();
        let mut out = Vec::new();
        while av_read_frame(fmt, pkt) >= 0 {
            conv.push(pkt, pts_ns_opt(pkt, st), &mut out).expect("a steady source converts");
            av_packet_unref(pkt);
        }
        let mut pkt = pkt;
        av_packet_free(&mut pkt);
        (conv.desc.clone(), out)
    });
    avformat_close_input(&mut fmt);
    result
}

/// What `ffprobe` reads in the AC-3 we produced: (codec, channels, rate, decoded seconds).
fn probe_ac3(dir: &std::path::Path, frames: &[(i64, Vec<u8>)]) -> (String, u32, u32, f64) {
    let path = dir.join("out.ac3");
    std::fs::write(&path, frames.iter().flat_map(|(_, b)| b.iter().copied()).collect::<Vec<u8>>()).unwrap();
    let out = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-count_packets", "-show_entries", "stream=codec_name,channels,sample_rate,nb_read_packets", "-of", "csv=p=0"])
        .arg(&path)
        .output()
        .expect("ffprobe beside ffmpeg");
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let f: Vec<&str> = line.split(',').collect();
    let (rate, packets): (u32, u32) = (f[1].parse().unwrap(), f[3].parse().unwrap());
    (f[0].to_string(), f[2].parse().unwrap(), rate, f64::from(packets) * 1536.0 / f64::from(rate))
}

#[test]
#[ignore = "requires the bundled host FFmpeg; run make check-ffmpeg"]
fn lossless_sources_convert_to_ac3_that_a_decoder_accepts() {
    let _guard = nj_base::testlock::serial();
    super::image_subtitle_tests::bind_host_ffmpeg();
    let dir = std::env::temp_dir().join(format!("nj-audio-convert-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        // FFmpeg's TrueHD ENCODER stops at 5.1 (it quietly writes 7.1 as 5.0), so the 7.1 fold
        // is exercised with LPCM, which carries 7.1 as written.
        ("thd51.mkv", "truehd", "5.1(side)", 48_000, 6, "TRUEHD 6ch -> AC3 5.1 640k"),
        ("pcm71.mkv", "pcm_s24le", "7.1", 48_000, 6, "PCM_S24LE 8ch -> AC3 5.1 640k"),
        ("flac20.mkv", "flac", "stereo", 44_100, 2, "FLAC 2ch -> AC3 stereo 256k"),
        ("pcm51.mkv", "pcm_s24le", "5.1(side)", 48_000, 6, "PCM_S24LE 6ch -> AC3 5.1 640k"),
        ("pcm10.mkv", "pcm_s16le", "mono", 32_000, 1, "PCM_S16LE 1ch -> AC3 mono 192k"),
    ];
    for (name, codec, layout, rate, out_channels, desc) in cases {
        let Some(path) = fixture(&dir, name, codec, layout, rate, 3) else { return };
        let (got_desc, frames) = unsafe { convert_file(&path) }.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(got_desc, desc);
        assert!(frames.windows(2).all(|w| w[1].0 > w[0].0), "{name}: timestamps rise");
        assert!(frames[0].0.abs() < 50_000_000, "{name}: the first frame is at the start, not {}", frames[0].0);
        let step = 1536 * 1_000_000_000 / i64::from(rate);
        assert!((frames[1].0 - frames[0].0 - step).abs() <= 1, "{name}: one AC-3 frame apart");
        let (got_codec, channels, got_rate, secs) = probe_ac3(&dir, &frames);
        assert_eq!((got_codec.as_str(), channels, got_rate), ("ac3", out_channels, rate), "{name}");
        assert!((secs - 3.0).abs() < 0.1, "{name}: {secs:.3} s of AC-3 for a 3 s source");
    }
    let Some(hi) = fixture(&dir, "flac96.mkv", "flac", "stereo", 96_000, 1) else { return };
    let refused = unsafe { convert_file(&hi) }.err().expect("96 kHz has no AC-3 rate");
    assert!(refused.contains("96000"), "{refused}");
    let _ = std::fs::remove_dir_all(&dir);
}
