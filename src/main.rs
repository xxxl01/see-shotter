//! 定时随机截图：在 total 分钟内，把时间切成每段 interval 分钟，
//! 每段内随机选一个时刻截全屏（含多显示器），文件名为 ISO 时间。
//! 用法: shotter [总分钟=60] [区间分钟=10] [输出目录=screenshots]

mod sched {
    /// 简单 xorshift64*，够用，免去 rand 依赖
    pub struct Rng(pub u64);
    impl Rng {
        pub fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545F4914F6CDD1D)
        }
    }

    /// 返回每个区间内的截图时刻（距开始的秒数），升序。
    pub fn plan(total_min: u64, interval_min: u64, rng: &mut Rng) -> Vec<u64> {
        let slot = interval_min * 60;
        (0..total_min / interval_min)
            .map(|k| k * slot + rng.next() % slot)
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn one_per_slot_within_slot() {
            for seed in 1..2000u64 {
                let mut r = Rng(seed * 7919);
                let p = plan(60, 10, &mut r);
                assert_eq!(p.len(), 6);
                for (k, t) in p.iter().enumerate() {
                    assert!(*t >= k as u64 * 600 && *t < (k as u64 + 1) * 600);
                }
            }
        }
        #[test]
        fn not_all_same_offset() {
            let mut r = Rng(12345);
            let a = plan(60, 10, &mut r);
            let offs: std::collections::HashSet<_> =
                a.iter().enumerate().map(|(k, t)| t - k as u64 * 600).collect();
            assert!(offs.len() > 1);
        }
    }
}

#[cfg(windows)]
mod win {
    use std::{fs::File, io::BufWriter, path::Path};
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::Performance::QueryPerformanceCounter;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub fn seed() -> u64 {
        let mut c = 0i64;
        unsafe { QueryPerformanceCounter(&mut c) };
        (c as u64) ^ 0x9E3779B97F4A7C15 | 1
    }

    pub fn init_dpi() {
        unsafe { windows_sys::Win32::UI::HiDpi::SetProcessDpiAwareness(2) };
    }

    /// ISO 8601 本地时间；Windows 文件名不能含 ':'，用 '-' 代替
    pub fn iso_name() -> String {
        let t = unsafe {
            let mut t = std::mem::zeroed();
            GetLocalTime(&mut t);
            t
        };
        format!(
            "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}.png",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
        )
    }

    pub fn capture(path: &Path) -> Result<(), String> {
        unsafe {
            let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
            let (w, h) = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
            if w <= 0 || h <= 0 {
                return Err("无法获取屏幕尺寸".into());
            }
            let sdc = GetDC(std::ptr::null_mut());
            let mdc = CreateCompatibleDC(sdc);
            let bmp = CreateCompatibleBitmap(sdc, w, h);
            let old = SelectObject(mdc, bmp as _);
            let ok = BitBlt(mdc, 0, 0, w, h, sdc, x, y, SRCCOPY | CAPTUREBLT);

            let mut buf = vec![0u8; (w * h * 4) as usize];
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = -h; // 自上而下
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;
            let lines = GetDIBits(mdc, bmp, 0, h as u32, buf.as_mut_ptr() as _, &mut bi, DIB_RGB_COLORS);

            SelectObject(mdc, old);
            DeleteObject(bmp as _);
            DeleteDC(mdc);
            ReleaseDC(std::ptr::null_mut(), sdc);
            if ok == 0 || lines == 0 {
                return Err("BitBlt/GetDIBits 失败".into());
            }

            // BGRA -> RGB，原地压缩（目标下标永远 <= 源下标），省一块内存
            let n = (w * h) as usize;
            for i in 0..n {
                let (b, g, r) = (buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2]);
                buf[i * 3] = r;
                buf[i * 3 + 1] = g;
                buf[i * 3 + 2] = b;
            }
            buf.truncate(n * 3);

            let f = File::create(path).map_err(|e| e.to_string())?;
            let mut enc = png::Encoder::new(BufWriter::new(f), w as u32, h as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::Fast);
            let mut wr = enc.write_header().map_err(|e| e.to_string())?;
            wr.write_image_data(&buf).map_err(|e| e.to_string())
        }
    }
}

#[cfg(windows)]
fn main() {
    use std::{path::PathBuf, thread::sleep, time::{Duration, Instant}};
    let a: Vec<String> = std::env::args().collect();
    let num = |i: usize, d: u64| a.get(i).and_then(|s| s.parse().ok()).filter(|&v| v > 0).unwrap_or(d);
    let (total, interval) = (num(1, 60), num(2, 10));
    let dir = PathBuf::from(a.get(3).map(String::as_str).unwrap_or("screenshots"));
    std::fs::create_dir_all(&dir).expect("无法创建输出目录");
    win::init_dpi();

    let mut rng = sched::Rng(win::seed());
    let start = Instant::now();
    for t in sched::plan(total, interval, &mut rng) {
        let target = Duration::from_secs(t);
        if let Some(wait) = target.checked_sub(start.elapsed()) {
            sleep(wait);
        }
        let p = dir.join(win::iso_name());
        if let Err(e) = win::capture(&p) {
            eprintln!("截图失败: {e}");
        }
    }
}

#[cfg(not(windows))]
fn main() {
    // 非 Windows 平台仅打印计划，便于验证调度逻辑
    let mut r = sched::Rng(0xDEADBEEF);
    for t in sched::plan(60, 10, &mut r) {
        println!("{:>2}m{:02}s", t / 60, t % 60);
    }
}
