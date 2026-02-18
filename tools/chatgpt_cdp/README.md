# ChatGPT CDP Runner

用 Python + Playwright 连接本地 Chrome（复用本机登录态）发送问题并保存回答。

## 目录结构
- `run.py`: 主入口
- `chrome_cdp.py`: Chrome/CDP 启动与 profile 处理
- `chatgpt_page.py`: ChatGPT 页面操作（打开、提问、等待回答）
- `output_writer.py`: 输出保存

## 依赖
```bash
pip3 install --user playwright
```

## 运行
```bash
python3 /Users/dingli/Desktop/GitHubProjects/kiro.rs/tools/chatgpt_cdp/run.py \
  --start-chrome \
  --question "请只回复：OK" \
  --output /Users/dingli/Desktop/GitHubProjects/kiro.rs/tools/chatgpt_cdp/output/answer.md
```

## 可选参数
- `--profile-directory Default`
- `--close-tab`
- `--cleanup-clone`
- `--output xxx.json`（保存 JSON）
