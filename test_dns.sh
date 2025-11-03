#!/bin/bash
# DNS和连接测试脚本

SERVER_ADDR="${1:-127.0.0.1:8443}"
PASSWORD="${2:-test_password}"

echo "=========================================="
echo "AnyTLS-RS DNS和连接测试"
echo "=========================================="
echo ""

# 解析服务器地址
IFS=':' read -r SERVER_HOST SERVER_PORT <<< "$SERVER_ADDR"
echo "服务器地址: $SERVER_ADDR"
echo "  主机: $SERVER_HOST"
echo "  端口: $SERVER_PORT"
echo ""

# 步骤1: DNS解析测试
echo "[1/5] DNS解析测试..."
if [[ "$SERVER_HOST" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || [[ "$SERVER_HOST" =~ ^\[.*\]$ ]]; then
    echo "  ✓ 使用IP地址，跳过DNS解析"
else
    echo "  → 解析域名: $SERVER_HOST"
    if command -v nslookup > /dev/null; then
        if nslookup "$SERVER_HOST" > /dev/null 2>&1; then
            echo "  ✓ DNS解析成功"
            nslookup "$SERVER_HOST" | head -5
        else
            echo "  ✗ DNS解析失败"
            echo "    建议：使用IP地址代替域名"
        fi
    elif command -v host > /dev/null; then
        if host "$SERVER_HOST" > /dev/null 2>&1; then
            echo "  ✓ DNS解析成功"
            host "$SERVER_HOST"
        else
            echo "  ✗ DNS解析失败"
            echo "    建议：使用IP地址代替域名"
        fi
    else
        echo "  ⚠ 无法测试DNS解析（未安装nslookup或host）"
    fi
fi

echo ""

# 步骤2: Ping测试（如果使用IP地址）
if [[ "$SERVER_HOST" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "[2/5] Ping测试..."
    if ping -c 1 -W 2 "$SERVER_HOST" > /dev/null 2>&1; then
        echo "  ✓ 主机可达"
    else
        echo "  ✗ 主机不可达（可能禁用了ICMP或网络不通）"
    fi
else
    echo "[2/5] Ping测试..."
    echo "  ⚠ 使用域名，跳过ping测试"
fi

echo ""

# 步骤3: TCP连接测试
echo "[3/5] TCP连接测试..."
if command -v nc > /dev/null; then
    if nc -zv -w 3 "$SERVER_HOST" "$SERVER_PORT" 2>&1 | grep -q "succeeded\|open"; then
        echo "  ✓ TCP连接成功"
    else
        echo "  ✗ TCP连接失败"
        echo "    检查："
        echo "    1. 服务器是否运行？"
        echo "    2. 端口是否正确？"
        echo "    3. 防火墙是否阻止？"
        nc -zv -w 3 "$SERVER_HOST" "$SERVER_PORT" 2>&1
    fi
elif command -v telnet > /dev/null; then
    timeout 3 telnet "$SERVER_HOST" "$SERVER_PORT" 2>&1 | head -3
    if [ $? -eq 0 ]; then
        echo "  ✓ TCP连接可能成功"
    else
        echo "  ✗ TCP连接失败"
    fi
else
    echo "  ⚠ 无法测试TCP连接（未安装nc或telnet）"
fi

echo ""

# 步骤4: 检查服务器进程
echo "[4/5] 检查服务器进程..."
if pgrep -f "anytls-server" > /dev/null; then
    echo "  ✓ 发现anytls-server进程"
    ps aux | grep "anytls-server" | grep -v grep | head -2
else
    echo "  ✗ 未发现anytls-server进程"
    echo "    确保服务器已启动"
fi

echo ""

# 步骤5: 端口监听检查
echo "[5/5] 端口监听检查..."
if command -v netstat > /dev/null; then
    if netstat -an 2>/dev/null | grep -q ":$SERVER_PORT.*LISTEN"; then
        echo "  ✓ 端口 $SERVER_PORT 正在监听"
        netstat -an 2>/dev/null | grep ":$SERVER_PORT.*LISTEN"
    else
        echo "  ✗ 端口 $SERVER_PORT 未监听"
    fi
elif command -v ss > /dev/null; then
    if ss -an 2>/dev/null | grep -q ":$SERVER_PORT.*LISTEN"; then
        echo "  ✓ 端口 $SERVER_PORT 正在监听"
        ss -an 2>/dev/null | grep ":$SERVER_PORT.*LISTEN"
    else
        echo "  ✗ 端口 $SERVER_PORT 未监听"
    fi
else
    echo "  ⚠ 无法检查端口监听（未安装netstat或ss）"
fi

echo ""
echo "=========================================="
echo "测试完成"
echo "=========================================="
echo ""
echo "如果所有测试通过，可以尝试启动客户端："
echo "  ./anytls-client -l 0.0.0.0:1080 -s $SERVER_ADDR -p $PASSWORD"

