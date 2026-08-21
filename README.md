# HomeNet

HomeNet 是一个基于 Tauri 的本地网络管理工具，前端使用 React + TypeScript 实现。

## WebDAV 服务

HomeNet 可以把一个本地目录作为 WebDAV 服务共享，并支持：

- 自定义监听地址和端口，默认仅监听 `127.0.0.1:19080`。
- 多账号、账号启停和独立的只读/读写权限。
- 全局只读模式、WebDAV 文件锁和服务运行状态检测。
- 密码使用 Argon2id 加盐哈希保存，不会在界面或 IPC 中回显。

WebDAV 使用 Basic Auth。通过公网访问时，请保持 WebDAV 仅监听本机，并使用 HomeNet 的 HTTPS 反向代理提供加密入口；不要直接把未加密的 WebDAV HTTP 端口暴露到公网。

## macOS 安装提示

如果 macOS 提示应用来自未知开发者，或下载后的应用被系统隔离，可以在安装到 `/Applications` 后执行：

```bash
sudo xattr -dr com.apple.quarantine /Applications/HomeNet.app
```
