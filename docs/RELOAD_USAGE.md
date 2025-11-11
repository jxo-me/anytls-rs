# TLS 证书重载使用指南

## 功能说明

服务器支持以下三种方式重载 TLS 证书：

1. **自动文件监听** - 监控证书文件变化，自动重载
2. **手动信号重载** - 发送 SIGHUP 信号触发重载
3. **证书到期检查** - 定期检查证书有效期并告警

## 使用方法

### 1. 启动服务器

```bash
# 基础启动（使用证书文件）
anytls-server -p mypassword --cert cert.pem --key key.pem

# 启用文件监听
anytls-server -p mypassword --cert cert.pem --key key.pem --watch-cert

# 显示证书信息
anytls-server -p mypassword --cert cert.pem --key key.pem --show-cert-info

# 自定义到期告警天数
anytls-server -p mypassword --cert cert.pem --key key.pem --expiry-warning-days 7
```

### 2. 手动重载证书

当证书文件更新后，可以通过发送 SIGHUP 信号手动触发重载：

```bash
# 查找服务器进程 PID
ps aux | grep anytls-server

# 发送 SIGHUP 信号
kill -HUP <pid>

# 或使用 killall
killall -HUP anytls-server
```

### 3. 自动文件监听

启用 `--watch-cert` 后，服务器会自动监控证书文件变化：

```bash
anytls-server -p mypassword --cert cert.pem --key key.pem --watch-cert
```

当检测到文件变化时，会自动重载证书，日志示例：

```
[CertReloader] File change detected, reloading certificate...
[CertReloader] Certificate reloaded successfully
```

### 4. 证书信息查看

使用 `--show-cert-info` 在启动时显示证书详细信息：

```bash
anytls-server -p mypassword --cert cert.pem --key key.pem --show-cert-info
```

输出示例：

```
=== TLS Certificate Information ===

Subject: CN=example.com
Issuer: CN=Let's Encrypt Authority
Serial: 1234567890abcdef
Valid From: 2025-01-01 00:00:00 UTC
Valid Until: 2025-04-01 00:00:00 UTC
Days Until Expiry: 80
Status: Valid

Subject Alternative Names:
  - example.com
  - www.example.com

Signature Algorithm: ECDSA with SHA-256
Public Key Algorithm: EC (256 bits)
Self-Signed: No

===================================
```

## 运行示例

### 完整配置启动

```bash
anytls-server \
  -p mypassword \
  -l 0.0.0.0:8443 \
  --cert /etc/ssl/cert.pem \
  --key /etc/ssl/key.pem \
  --watch-cert \
  --show-cert-info \
  --expiry-warning-days 30 \
  -L info
```

### 证书更新流程

```bash
# 1. 更新证书文件（例如使用 certbot）
certbot renew

# 2a. 如果启用了 --watch-cert，会自动重载

# 2b. 或手动发送信号触发重载
killall -HUP anytls-server

# 3. 查看日志确认重载成功
# [Server] SIGHUP received, reloading certificates...
# [Server] Certificate reload successful
```

## 注意事项

- 证书重载是原子操作，不会中断现有连接
- 新连接会自动使用新证书
- 如果新证书无效，会保留旧证书并记录错误
- 自动文件监听有 500ms 的防抖延迟
- 证书到期检查每小时执行一次

