# agy-auth

`agy-auth` 是一个用于 Antigravity CLI (`agy`) 的多账号切换及凭据同步命令行工具。它直接对接本地的账号存储体系 (`~/.agy_auth`)。

---

## 1. 安装方法 

其它用户只需在安装了 Node.js / npm 的任何电脑上直接运行即可：

```bash
# 全局安装，支持 macOS、Windows、Linux
npm install -g agy-auth
```

支持的平台：macOS x64 / ARM64、Windows x64、Linux x64 / ARM64（aarch64，glibc）。
npm 会自动选择当前平台和架构对应的预编译包。
Linux ARM64 支持将在包含此改动的新版本发布后提供；不适用于 Android / Termux 或 Alpine / musl。
Linux 切换系统凭据需要安装 `secret-tool`（Debian/Ubuntu 包名为 `libsecret-tools`），并运行可用的 Secret Service。

GitHub Release 同时提供 `agy-auth-linux-aarch64.tar.gz`，解压后可直接运行。

或者使用 `npx` 临时免安装执行：

```bash
npx agy-auth list
```

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

