# TLS 证书热重载使用指南

## 简介

AnyTLS 支持 TLS 证书的热重载功能，可以在不重启服务的情况下更新证书。这对于生产环境和 Let's Encrypt 自动续期场景特别有用。

## 功能特性

✅ **文件监控**: 自动监控证书和密钥文件的变化  
✅ **热重载**: 检测到变化后自动重新加载，无需重启  
✅ **证书分析**: 显示证书详细信息（主题、有效期、SANs等）  
✅ **过期检查**: 定期检查证书有效期并发出警告  
✅ **错误保护**: 新证书加载失败时保持使用旧证书  
✅ **零停机**: 现有连接不受影响，新连接使用新证书

## 快速开始

### 1. 基本使用

```bash
# 启用证书监控和自动重载
anytls-server \
  -p password \
  --cert /path/to/cert.pem \
  --key /path/to/key.pem \
  --watch-cert
```

### 2. 显示证书信息

```bash
# 启动时显示证书详细信息
anytls-server \
  -p password \
  --cert /path/to/cert.pem \
  --key /path/to/key.pem \
  --show-cert-info
```

输出示例：
```
=== TLS Certificate Information ===

Subject: CN=example.com, O=Example Inc, C=US
Issuer: CN=Let's Encrypt Authority X3, O=Let's Encrypt, C=US
Serial Number: 03:46:e3:7a:...
Valid From: 2025-01-01 00:00:00 UTC
Valid Until: 2025-04-01 00:00:00 UTC
Days Until Expiry: 45
Signature Algorithm: SHA256withRSA
Public Key Algorithm: RSA
Self-Signed: false
SANs: example.com, www.example.com

===================================
```

### 3. 配置过期警告

```bash
# 证书剩余 15 天时开始警告
anytls-server \
  -p password \
  --cert /path/to/cert.pem \
  --key /path/to/key.pem \
  --watch-cert \
  --expiry-warning-days 15
```

## 完整示例

### Let's Encrypt 自动续期

```bash
# 服务器配置
anytls-server \
  -p mypassword \
  -l 0.0.0.0:443 \
  --cert /etc/letsencrypt/live/example.com/fullchain.pem \
  --key /etc/letsencrypt/live/example.com/privkey.pem \
  --watch-cert \
  --expiry-warning-days 30 \
  -L info
```

当 Let's Encrypt 证书自动续期后，AnyTLS 会：
1. 检测到证书文件变化
2. 等待防抖时间（避免频繁重载）
3. 重新加载新证书
4. 验证证书有效性
5. 原子性切换到新证书
6. 记录重载操作

### 手动证书更新

```bash
# 1. 启动服务器
anytls-server \
  -p password \
  --cert ./certs/server.pem \
  --key ./certs/server-key.pem \
  --watch-cert

# 2. 更新证书（在另一个终端）
cp new-cert.pem ./certs/server.pem
cp new-key.pem ./certs/server-key.pem

# 服务器会自动检测并重载
# 日志输出：
# [INFO] File change detected, reloading...
# [INFO] Reloading certificate...
# [INFO] Certificate changed: CN=old.example.com -> CN=new.example.com
# [INFO] Certificate reload completed in 125ms
```

## 日志示例

### 正常启动
```
anytls-server v0.4.1
Listening on 0.0.0.0:8443
[INFO] [CertReloader] Initial certificate loaded: CN=example.com, expires in 45 days
[INFO] [CertReloader] Starting file watcher for: "/etc/ssl/cert.pem" and "/etc/ssl/key.pem"
[Server] New connection from 192.168.1.100
```

### 证书重载
```
[DEBUG] [CertReloader] File change detected, reloading...
[INFO] [CertReloader] Reloading certificate...
[INFO] [CertReloader] Certificate changed: CN=example.com (old) -> CN=example.com (new)
[INFO] [CertReloader] New certificate: CN=example.com, expires in 90 days
[INFO] [CertReloader] Certificate reload completed in 125ms
```

### 过期警告
```
[WARN] [CertReloader] WARNING: Certificate expiring in 28 days! CN=example.com
[INFO] Please renew certificate before: 2025-04-15 00:00:00 UTC
```

### 重载失败
```
[DEBUG] [CertReloader] File change detected, reloading...
[ERROR] [CertReloader] Failed to reload certificate: invalid PEM format
[WARN] [CertReloader] Keeping current certificate active
```

## 配置选项

### 命令行参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--cert FILE` | 证书文件路径 | - |
| `--key FILE` | 私钥文件路径 | - |
| `--watch-cert` | 启用证书文件监控 | false |
| `--show-cert-info` | 显示证书信息 | false |
| `--expiry-warning-days DAYS` | 过期警告阈值（天） | 30 |

### 环境变量

```bash
# 设置日志级别以查看详细的重载信息
export RUST_LOG=anytls_rs::util::cert_reloader=debug

# 或只查看 info 及以上级别
export RUST_LOG=info

# 启动服务器
anytls-server -p password --cert cert.pem --key key.pem --watch-cert
```

## 最佳实践

### 1. 生产环境配置

```bash
#!/bin/bash
# production-server.sh

anytls-server \
  -p "${SERVER_PASSWORD}" \
  -l "0.0.0.0:443" \
  --cert "/etc/anytls/certs/server.pem" \
  --key "/etc/anytls/certs/server-key.pem" \
  --watch-cert \
  --expiry-warning-days 30 \
  --log-level info \
  2>&1 | tee -a /var/log/anytls/server.log
```

### 2. 证书文件权限

```bash
# 设置正确的文件权限
chmod 600 /etc/anytls/certs/server-key.pem
chmod 644 /etc/anytls/certs/server.pem

# 确保 anytls 进程有读取权限
chown anytls:anytls /etc/anytls/certs/*.pem
```

### 3. Systemd 服务

```ini
# /etc/systemd/system/anytls-server.service
[Unit]
Description=AnyTLS Server
After=network.target

[Service]
Type=simple
User=anytls
Group=anytls
ExecStart=/usr/local/bin/anytls-server \
  -p password \
  --cert /etc/anytls/certs/server.pem \
  --key /etc/anytls/certs/server-key.pem \
  --watch-cert \
  --expiry-warning-days 30
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

启动服务：
```bash
sudo systemctl daemon-reload
sudo systemctl enable anytls-server
sudo systemctl start anytls-server

# 查看日志
journalctl -u anytls-server -f
```

### 4. Let's Encrypt 集成

```bash
# certbot 续期钩子
# /etc/letsencrypt/renewal-hooks/deploy/anytls-reload.sh

#!/bin/bash
DOMAIN="example.com"
CERT_DIR="/etc/letsencrypt/live/$DOMAIN"
ANYTLS_CERT_DIR="/etc/anytls/certs"

# 复制新证书
cp "$CERT_DIR/fullchain.pem" "$ANYTLS_CERT_DIR/server.pem"
cp "$CERT_DIR/privkey.pem" "$ANYTLS_CERT_DIR/server-key.pem"

# 设置权限
chmod 600 "$ANYTLS_CERT_DIR/server-key.pem"
chmod 644 "$ANYTLS_CERT_DIR/server.pem"
chown anytls:anytls "$ANYTLS_CERT_DIR"/*.pem

echo "Certificate copied, AnyTLS will auto-reload"
```

设置权限：
```bash
chmod +x /etc/letsencrypt/renewal-hooks/deploy/anytls-reload.sh
```

测试续期：
```bash
sudo certbot renew --dry-run
```

## 故障排查

### 问题 1: 证书重载失败

**症状**: 日志显示 "Failed to reload certificate"

**原因**: 
- 证书文件格式错误
- 私钥与证书不匹配
- 文件权限问题
- 证书已过期

**解决方法**:
```bash
# 1. 验证证书格式
openssl x509 -in cert.pem -text -noout

# 2. 验证私钥格式
openssl rsa -in key.pem -check

# 3. 验证证书和私钥是否匹配
openssl x509 -noout -modulus -in cert.pem | openssl md5
openssl rsa -noout -modulus -in key.pem | openssl md5
# 输出的 MD5 值应该相同

# 4. 检查文件权限
ls -l cert.pem key.pem
```

### 问题 2: 文件监控不工作

**症状**: 更新证书文件后没有自动重载

**原因**:
- 没有启用 `--watch-cert` 参数
- 文件系统不支持 inotify（某些网络文件系统）
- 文件被移动而不是覆盖

**解决方法**:
```bash
# 1. 确保使用了 --watch-cert
anytls-server -p password --cert cert.pem --key key.pem --watch-cert

# 2. 使用覆盖而不是移动
cp new-cert.pem cert.pem  # ✓ 正确
mv new-cert.pem cert.pem  # ✗ 可能不触发监控

# 3. 查看详细日志
RUST_LOG=debug anytls-server --watch-cert ...
```

### 问题 3: 证书过期警告

**症状**: 日志显示 "Certificate expiring in X days"

**解决方法**:
```bash
# 1. 检查证书有效期
openssl x509 -in cert.pem -noout -dates

# 2. 续期证书（Let's Encrypt）
sudo certbot renew

# 3. 手动更新证书
# 将新证书复制到相应位置，AnyTLS 会自动重载
```

## 性能影响

### 资源使用
- **内存**: < 1MB (包含文件监控)
- **CPU**: < 0.1% (空闲时)
- **重载时间**: 50-200ms

### 对服务的影响
- **现有连接**: 无影响（继续使用旧证书）
- **新连接**: 立即使用新证书
- **停机时间**: 0（零停机）

## 安全建议

1. **保护私钥文件**
   ```bash
   chmod 600 /path/to/key.pem
   chown root:root /path/to/key.pem
   ```

2. **定期检查证书**
   - 设置合理的过期警告时间
   - 建立监控和告警机制
   - 定期审查证书日志

3. **备份证书**
   ```bash
   # 定期备份证书和密钥
   tar -czf certs-backup-$(date +%Y%m%d).tar.gz /etc/anytls/certs/
   ```

4. **审计日志**
   ```bash
   # 记录所有证书变更
   journalctl -u anytls-server | grep CertReloader > cert-audit.log
   ```

## 相关资源

- [设计文档](./TLS_CERT_RELOAD_DESIGN.md) - 详细的技术设计
- [API 文档](https://docs.rs/anytls-rs) - Rust API 文档
- [Let's Encrypt 文档](https://letsencrypt.org/docs/) - 证书自动续期
- [故障排查指南](./TROUBLESHOOTING.md) - 常见问题解决

## 示例代码

### Rust API 使用

```rust
use anytls_rs::util::{CertReloader, CertReloaderConfig};
use std::sync::Arc;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 配置证书重载器
    let config = CertReloaderConfig {
        cert_path: PathBuf::from("/etc/ssl/cert.pem"),
        key_path: PathBuf::from("/etc/ssl/key.pem"),
        watch_enabled: true,
        debounce_ms: 500,
        check_expiry: true,
        expiry_warning_days: 30,
    };

    // 创建证书重载器
    let reloader = Arc::new(CertReloader::new(config)?);

    // 显示当前证书信息
    reloader.show_cert_info();

    // 启动文件监控
    reloader.clone().start_watching()?;

    // 启动过期检查（每小时检查一次）
    reloader.clone().start_expiry_checker(
        tokio::time::Duration::from_secs(3600)
    );

    // 获取 TLS acceptor 用于服务器
    let acceptor = reloader.get_acceptor();

    // 使用 acceptor 接受连接...

    Ok(())
}
```

## 总结

TLS 证书热重载功能让 AnyTLS 更适合生产环境使用：

✅ **自动化**: 无需手动干预即可更新证书  
✅ **可靠**: 新证书失败时保持旧证书  
✅ **零停机**: 不影响现有连接  
✅ **易用**: 简单的命令行参数即可启用  
✅ **安全**: 证书验证和过期检查

如有问题或建议，欢迎反馈！

