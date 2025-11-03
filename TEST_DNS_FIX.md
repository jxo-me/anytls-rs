# DNS解析错误修复指南

## 错误信息

```
ERROR [SOCKS5] Failed to create proxy stream: IO error: failed to lookup address information: Try again
```

## 问题分析

这个错误表明客户端在尝试连接到服务器时，DNS解析失败。可能的原因：

1. **服务器地址格式错误**
   - 使用了域名但DNS无法解析
   - 使用了IP地址但格式不正确

2. **服务器未运行**
   - 服务器进程未启动
   - 服务器监听地址配置错误

3. **网络问题**
   - DNS服务器不可用
   - 网络连接问题

## 修复步骤

### 步骤1: 验证服务器地址格式

服务器地址应该格式为 `IP:PORT` 或 `HOSTNAME:PORT`

```bash
# 正确格式示例
127.0.0.1:8443          # IPv4
[::1]:8443              # IPv6
server.example.com:8443 # 域名
```

### 步骤2: 测试DNS解析

```bash
# 如果使用域名，先测试DNS解析
nslookup server_ip
# 或
dig server_ip
# 或
host server_ip
```

### 步骤3: 测试网络连通性

```bash
# 测试TCP连接（假设端口8443）
telnet server_ip 8443
# 或
nc -zv server_ip 8443
```

### 步骤4: 检查服务器是否运行

```bash
# 检查服务器进程
ps aux | grep anytls-server

# 检查端口监听
netstat -an | grep 8443
# 或
ss -an | grep 8443
```

### 步骤5: 使用IP地址代替域名

如果DNS解析有问题，直接使用IP地址：

```bash
# 使用IP地址启动客户端
./anytls-client -l 0.0.0.0:1080 -s 192.168.1.100:8443 -p password
```

## 代码改进

已添加更详细的错误日志：
- DNS解析失败时会明确提示
- 显示正在尝试连接的服务器地址
- 提供故障排查建议

## 调试命令

```bash
# 使用详细日志运行客户端
RUST_LOG=debug ./anytls-client -l 0.0.0.0:1080 -s server_ip:8443 -p password

# 查看完整的连接过程
RUST_LOG=trace ./anytls-client -l 0.0.0.0:1080 -s server_ip:8443 -p password
```

## 常见问题

### Q: 服务器地址应该使用什么格式？
A: 使用 `IP:PORT` 或 `HOSTNAME:PORT`，例如：
- `127.0.0.1:8443` (本地IPv4)
- `192.168.1.100:8443` (局域网IPv4)
- `server.example.com:8443` (域名)

### Q: 为什么会出现"Try again"错误？
A: 这通常表示DNS查询超时或失败。可能原因：
1. DNS服务器不可达
2. 域名不存在
3. 网络连接问题

### Q: 如何确认服务器地址是正确的？
A: 
1. 使用 `telnet` 或 `nc` 测试连接
2. 检查服务器日志确认监听地址
3. 使用 `ping` 测试基本连通性（如果服务器允许ICMP）

