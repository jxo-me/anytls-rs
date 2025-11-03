#!/bin/bash
set -e

PASSWORD="test_password"
SERVER_PORT=8443
CLIENT_PORT=1080
TEST_URL="http://httpbin.org/get"

echo "=== AnyTLS-RS 端到端测试 ==="

# 1. 编译
echo "[1/6] 编译二进制文件..."
cargo build --release --bins
if [ $? -ne 0 ]; then
    echo "✗ 编译失败"
    exit 1
fi
echo "✓ 编译成功"

# 2. 启动服务器（后台）
echo "[2/6] 启动服务器..."
cargo run --release --bin anytls-server -- \
  -l "127.0.0.1:$SERVER_PORT" \
  -p "$PASSWORD" > server.log 2>&1 &
SERVER_PID=$!
sleep 3

# 检查服务器是否启动成功
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "✗ 服务器启动失败"
    cat server.log
    exit 1
fi
echo "✓ 服务器启动成功 (PID: $SERVER_PID)"

# 3. 启动客户端（后台）
echo "[3/6] 启动客户端..."
cargo run --release --bin anytls-client -- \
  -l "127.0.0.1:$CLIENT_PORT" \
  -s "127.0.0.1:$SERVER_PORT" \
  -p "$PASSWORD" > client.log 2>&1 &
CLIENT_PID=$!
sleep 3

# 检查客户端是否启动成功
if ! kill -0 $CLIENT_PID 2>/dev/null; then
    echo "✗ 客户端启动失败"
    cat client.log
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi
echo "✓ 客户端启动成功 (PID: $CLIENT_PID)"

# 4. 测试SOCKS5连接
echo "[4/6] 测试SOCKS5代理..."
for i in {1..5}; do
    if curl -s --connect-timeout 5 --socks5-hostname "127.0.0.1:$CLIENT_PORT" "$TEST_URL" > /dev/null 2>&1; then
        echo "✓ SOCKS5代理测试通过 (尝试 $i/5)"
        break
    else
        if [ $i -eq 5 ]; then
            echo "✗ SOCKS5代理测试失败 (5次尝试均失败)"
            echo "服务器日志:"
            tail -20 server.log
            echo "客户端日志:"
            tail -20 client.log
            kill $CLIENT_PID 2>/dev/null || true
            kill $SERVER_PID 2>/dev/null || true
            exit 1
        fi
        echo "  重试中 ($i/5)..."
        sleep 2
    fi
done

# 5. 测试多次请求
echo "[5/6] 测试多次请求..."
SUCCESS_COUNT=0
for i in {1..3}; do
    if curl -s --connect-timeout 5 --socks5-hostname "127.0.0.1:$CLIENT_PORT" "$TEST_URL" > /dev/null 2>&1; then
        SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
    fi
done
if [ $SUCCESS_COUNT -eq 3 ]; then
    echo "✓ 多次请求测试通过 ($SUCCESS_COUNT/3)"
else
    echo "⚠ 多次请求测试部分成功 ($SUCCESS_COUNT/3)"
fi

# 6. 清理
echo "[6/6] 清理进程..."
kill $CLIENT_PID 2>/dev/null || true
sleep 1
kill $SERVER_PID 2>/dev/null || true
sleep 1

# 检查日志中的错误
echo ""
echo "=== 日志检查 ==="
if grep -qi "error\|panic\|fatal" server.log client.log 2>/dev/null; then
    echo "⚠ 发现错误日志:"
    grep -i "error\|panic\|fatal" server.log client.log 2>/dev/null | head -10
else
    echo "✓ 无严重错误日志"
fi

echo ""
echo "=== 测试完成 ==="
echo "服务器日志: server.log"
echo "客户端日志: client.log"

