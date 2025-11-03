#!/bin/bash
# 简单的手动测试脚本

PASSWORD="test_password"

echo "=== 步骤1: 启动服务器（后台）==="
cargo run --release --bin anytls-server -- -l 127.0.0.1:8443 -p "$PASSWORD" &
SERVER_PID=$!
echo "服务器PID: $SERVER_PID"
sleep 3

echo ""
echo "=== 步骤2: 启动客户端（后台）==="
cargo run --release --bin anytls-client -- -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p "$PASSWORD" &
CLIENT_PID=$!
echo "客户端PID: $CLIENT_PID"
sleep 5

echo ""
echo "=== 步骤3: 测试SOCKS5代理 ==="
echo "等待连接建立..."
sleep 2

curl -v --socks5-hostname 127.0.0.1:1080 --connect-timeout 10 http://httpbin.org/get 2>&1 | head -30

echo ""
echo "=== 清理 ==="
kill $CLIENT_PID $SERVER_PID 2>/dev/null
wait $CLIENT_PID $SERVER_PID 2>/dev/null
echo "测试完成"
