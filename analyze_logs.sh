#!/bin/bash
# 日志分析脚本

if [ $# -lt 2 ]; then
    echo "用法: $0 <server.log> <client.log>"
    exit 1
fi

SERVER_LOG=$1
CLIENT_LOG=$2

echo "=========================================="
echo "AnyTLS-RS 日志分析"
echo "=========================================="
echo ""

# 检查关键事件时间线
echo "=== 连接建立时间线 ==="
echo ""
echo "服务器端:"
grep -E "New connection|TLS handshake|Authentication|Connection established|Received SYN|Sending SYNACK|Handling stream" "$SERVER_LOG" | head -20

echo ""
echo "客户端端:"
grep -E "Creating new session|TLS handshake|Authentication|Client session started|Creating proxy stream|SYNACK received" "$CLIENT_LOG" | head -20

echo ""
echo "=== 错误统计 ==="
echo ""
echo "服务器错误:"
grep -i "error\|failed\|panic" "$SERVER_LOG" | grep -v "debug\|trace" | wc -l
grep -i "error\|failed\|panic" "$SERVER_LOG" | grep -v "debug\|trace" | head -10

echo ""
echo "客户端错误:"
grep -i "error\|failed\|panic" "$CLIENT_LOG" | grep -v "debug\|trace" | wc -l
grep -i "error\|failed\|panic" "$CLIENT_LOG" | grep -v "debug\|trace" | head -10

echo ""
echo "=== SYN/SYNACK 流程检查 ==="
echo ""
echo "服务器发送的SYNACK:"
grep "Sending SYNACK" "$SERVER_LOG"

echo ""
echo "客户端收到的SYNACK:"
grep "SYNACK received\|Received SYNACK" "$CLIENT_LOG"

echo ""
echo "=== 数据流检查 ==="
echo ""
echo "PSH帧统计:"
echo "  服务器收到: $(grep -c "Received PSH\|PSH frame" "$SERVER_LOG" 2>/dev/null || echo 0)"
echo "  客户端收到: $(grep -c "Received PSH\|PSH frame" "$CLIENT_LOG" 2>/dev/null || echo 0)"

echo ""
echo "=== 性能指标 ==="
echo ""
if grep -q "Handling stream" "$SERVER_LOG"; then
    echo "处理的流数量: $(grep -c "Handling stream" "$SERVER_LOG")"
fi

if grep -q "Proxy stream created" "$CLIENT_LOG"; then
    echo "创建的代理流数量: $(grep -c "Proxy stream created" "$CLIENT_LOG")"
fi

