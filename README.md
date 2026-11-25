# agy-auth

`agy-auth` 是一个用于 Antigravity CLI (`agy`) 的多账号切换及凭据同步命令行工具。它直接对接本地的账号存储体系 (`~/.agy_auth`)。

---

## 1. 安装方法 

从 [GitHub Releases](https://github.com/dwdcth/agy-auth/releases) 下载对应平台的压缩包，解压后将二进制放入 `PATH` 即可：

| 平台 | 资产文件 |
| --- | --- |
| Linux x64 | `agy-auth-linux-x86_64.tar.gz` |
| Linux ARM64 (aarch64) | `agy-auth-linux-aarch64.tar.gz` |
| Linux LoongArch64 | `agy-auth-linux-loongarch64.tar.gz` |
| macOS x64 (Intel) | `agy-auth-macos-x86_64.tar.gz` |
| macOS ARM64 (Apple Silicon) | `agy-auth-macos-aarch64.tar.gz` |
| Windows x64 | `agy-auth-windows-x86_64.zip` |

```bash
# 示例：Linux x64
curl -LO https://github.com/dwdcth/agy-auth/releases/latest/download/agy-auth-linux-x86_64.tar.gz
tar -xzf agy-auth-linux-x86_64.tar.gz
install -m 755 agy-auth-linux-x86_64 ~/.local/bin/agy-auth
```

或者使用 Cargo 从源码安装：

```bash
cargo install --git https://github.com/dwdcth/agy-auth --locked
```

支持的平台：macOS x64 / ARM64、Windows x64、Linux x64 / ARM64 / LoongArch64（glibc）。
不适用于 Android / Termux 或 Alpine / musl。

Linux 切换系统凭据的行为：存在可用的 Secret Service 时优先使用（需要 `secret-tool`，
Debian/Ubuntu 包名为 `libsecret-tools`）；headless 环境没有 Secret Service 时，自动
写入 AGY CLI 的 token 文件（`~/.gemini/antigravity-cli/antigravity-oauth-token`），
无需安装额外依赖。注意：正在运行的 `agy` 会话会缓存旧凭据，切换后需重启会话生效。

---

## 2. 命令行参考 (CLI Subcommands)

* **列出所有已登录账号**：
  ```bash
  agy-auth list  # 或者 agy-auth ls
  ```

* **添加账号（支持浏览器登录）**：
  ```bash
  # 启动本地服务器捕获浏览器 OAuth 登录回调
  agy-auth add   # 或者 agy-auth login
  
  # 或者直接传入已有的 refresh_token 进行静默添加
  agy-auth add "<refresh_token>"
  ```

* **切换激活账户**：
  ```bash
  agy-auth switch <account_email_or_id> # 或者 agy-auth use <email_or_id>
  ```
  切换时会同步系统凭据及 `~/.gemini/google_accounts.json`：更新 `active`，
  将原账号加入 `old`，移除目标账号的历史记录并去重，保留其他字段。
  缓存文件使用原子替换写入（Unix 权限 `0600`）；不存在时自动创建，格式损坏时不会覆盖。
  两项同步成功后才更新本工具的当前账号；若系统凭据已写入但缓存同步失败，会明确提示重试。

* **查看当前激活的账号详情**：
  ```bash
  agy-auth current  # 或者 agy-auth whoami
  ```

* **删除账号**：
  ```bash
  agy-auth delete <account_email_or_id> # 或者 agy-auth remove <email_or_id>
  ```

* **导入账号**：
  ```bash
  # 交互选择：本机 Antigravity 或 JSON 文件
  agy-auth import

  # 从本机当前 Antigravity / AGY 登录态导入
  # （按顺序尝试：系统钥匙串 → CLI token 文件 → IDE state.vscdb）
  agy-auth import --local

  # 从 JSON 文件导入（支持 cockpit / agy-auth 导出格式）
  agy-auth import ./accounts.json
  ```

  JSON 示例（简化格式，可单对象或数组）：

  ```json
  [
    {
      "email": "user@gmail.com",
      "refresh_token": "1//0e...",
      "tags": ["work"],
      "notes": "optional"
    }
  ]
  ```

