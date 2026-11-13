# see-shotter

Windows 定时随机截图小工具（Rust，单文件 exe，约 340 KB）。

把总时长切成若干区间，每个区间内随机选一个时刻截全屏（含多显示器），文件名为 ISO 本地时间，如 `2026-11-13T14-05-33.png`。

```
shotter.exe [总分钟=60] [区间分钟=10] [输出目录=screenshots]
```

- 下载：见 [Releases](../../releases)（推送 `v*` tag 后由 GitHub Actions 自动编译发布）
- 自行编译：`cargo build --release`（在 Windows 上）
