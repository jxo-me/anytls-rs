# Linux版本编译快速指南

## ⚡ 快速开始

```bash
# 1. 确保Docker运行
docker info

# 2. 运行编译脚本
./build-linux.sh
```

## 📋 编译结果

编译成功后，二进制文件位于：

```
target/x86_64-unknown-linux-musl/release/anytls-server
target/x86_64-unknown-linux-musl/release/anytls-client
```

## 📤 传输到Linux服务器

```bash
# 使用SCP
scp target/x86_64-unknown-linux-musl/release/anytls-server user@host:/usr/local/bin/
scp target/x86_64-unknown-linux-musl/release/anytls-client user@host:/usr/local/bin/
```

## 🐧 Linux服务器使用

```bash
# 设置权限
chmod +x /usr/local/bin/anytls-server
chmod +x /usr/local/bin/anytls-client

# 启动服务器
./anytls-server -l 0.0.0.0:8443 -p your_password

# 启动客户端
./anytls-client -l 127.0.0.1:1080 -s server_ip:8443 -p your_password
```

## ⚠️ 注意事项

1. **首次编译**: 需要下载Docker镜像，可能需要几分钟
2. **Docker要求**: 必须安装并运行Docker Desktop（macOS）或Docker（Linux）
3. **文件大小**: 每个二进制文件约6-7MB（静态链接）
4. **架构**: 当前编译的是x86_64版本

## 📖 更多信息

详细说明请参考 `LINUX_BUILD.md`

