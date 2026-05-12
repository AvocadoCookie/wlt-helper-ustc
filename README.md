# wlt-helper-ustc

面向 Linux 的 USTC 网络通登录助手

守护进程自动检测网络状态，在校园网环境下完成网络通认证，
无需每次手动登录。

## 安装

```bash
git clone https://github.com/AvocadoCookie/wlt-helper-ustc.git
cd wlt-helper-ustc
cargo build --release
```

编译产物：

- `target/release/wltsv` — 守护进程
- `target/release/wltcmd` — 命令行客户端

## 快速开始

### 1. 创建配置文件

```bash
mkdir -p ~/.config/wlt-helper
cp config.example.toml ~/.config/wlt-helper/config.toml
```

编辑 `~/.config/wlt-helper/config.toml`，填入你的用户名
（也可使用命令 `wltcmd set` 填入）：

```toml
name = "your_username"
```

其他选项可保留默认值，也可根据注释中的说明来自行调整。

### 2. 保存密码

```bash
wltcmd set
```

输入用户名和密码，密码将被安全存储在系统密钥环中。

### 3. 启动守护进程

```bash
wltsv &
```

守护进程启动后自动检测是否需要登录。此后每次网络切换
（插拔网线、切换 Wi-Fi）都会自动尝试重连网络通。

### 4. 设为开机自启（可选）

这里以 `systemctl` 为例。复制[服务配置文件](./wlt-helper.service)到 `~/.config/systemd/user/`
目录下，并将其中 `ExecStart` 后的路径改为 `wltsv` 的实际路径，
然后运行

```bash
systemctl --user daemon-reload
systemctl --user enable --now wlt-helper
```

启动该服务。

## 配置说明

配置文件位于 `${XDG_CONFIG_HOME}/wlt-helper/config.toml`，
若环境变量 `XDG_CONFIG_HOME` 不存在，则使用 `${HOME}/.config/wlt-helper/config.toml`

| 配置项 | 可选值 | 默认值 | 说明 |
| ------ | ------ | ------ | ---- |
| `name` | 字符串 | — | 网络通用户名（必填） |
| `type` | 0~8 | 0 | 网络通登录类型 |
| `exp` | 0, 3600, 14400, 39600, 50400 | 3600 | 出口时长（秒） |
| `check` | `"nm"`, `"ping"` | `"ping"` | [检测模式](#检测模式) |
| `ping.interval` | 正整数 | 3600 | ping 间隔（秒） |
| `ping.site` | URI | `https://www.baidu.com/` | 探测目标 |

## 命令行用法

```bash
wltcmd set    # 存储凭据
wltcmd login  # 手动触发一次登录检测
```

## 检测模式

### nm 模式（推荐）

通过 NetworkManager D-Bus 接口监听主连接变更。
切换网络时自动检测校园网环境并登录。
适合桌面 Linux 环境。

### ping 模式

定期向 `site` 发送 HEAD 请求，不可达时自动登录。
该方法不依赖 D-Bus。

## 工作原理

```
wltsv 守护进程
  ├── 读取配置，选择检测模式
  ├── nm 模式 ──→ D-Bus 监听 PrimaryConnection 变更
  └── ping 模式 → 定时 HEAD 探测目标站
        │
        ├── 检测到网络变更/不可达
        └── log_in()
              ├── 检查是否校园网（有线：IP 前缀 114.214）
              ├── 从 keyring 获取密码
              └── 提交 HTTP 表单到 wlt.ustc.edu.cn
```

## 依赖

- Rust 1.82+
- NetworkManager（nm 模式）
- 系统密钥环

## 许可证

MIT
