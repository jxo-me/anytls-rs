# 贡献指南

感谢您对 AnyTLS-RS 项目的兴趣！我们欢迎所有形式的贡献。

---

## 📋 目录

1. [行为准则](#行为准则)
2. [如何贡献](#如何贡献)
3. [开发环境设置](#开发环境设置)
4. [开发流程](#开发流程)
5. [代码规范](#代码规范)
6. [测试要求](#测试要求)
7. [文档要求](#文档要求)
8. [提交规范](#提交规范)
9. [Pull Request 流程](#pull-request-流程)

---

## 行为准则

### 我们的承诺

为了营造开放和友好的环境，我们承诺：

- 尊重所有贡献者
- 接受建设性的批评
- 关注社区最大利益
- 对他人展现同理心

### 不可接受的行为

- 使用性别化语言或图像
- 人身攻击或侮辱
- 公开或私下骚扰
- 未经授权发布他人隐私信息
- 其他不当行为

---

## 如何贡献

### 报告 Bug

如果您发现 bug，请：

1. 在 [GitHub Issues](https://github.com/yourusername/anytls-rs/issues) 搜索是否已存在
2. 如果不存在，创建新的 issue
3. 使用清晰的标题
4. 提供详细的复现步骤
5. 包含系统信息（OS、Rust 版本等）
6. 附上日志或错误信息

**Bug 报告模板**:

```markdown
**描述**
简要描述 bug

**复现步骤**
1. 执行命令 '...'
2. 连接到 '...'
3. 看到错误 '...'

**预期行为**
应该发生什么

**实际行为**
实际发生了什么

**环境**
- OS: [e.g., Windows 10, Ubuntu 22.04]
- Rust 版本: [e.g., 1.70.0]
- AnyTLS-RS 版本: [e.g., 0.2.0]

**日志**
```
粘贴相关日志
```

**额外信息**
其他有用的信息
```

### 提出功能请求

1. 搜索现有的 feature requests
2. 创建新的 issue，标记为 "enhancement"
3. 清晰描述建议的功能
4. 解释为什么需要这个功能
5. 提供使用示例（如果可能）

### 贡献代码

我们欢迎 Pull Requests！请参考下面的开发流程。

### 改进文档

- 修正拼写错误
- 改进说明清晰度
- 添加示例
- 翻译文档

---

## 开发环境设置

### 前置要求

- **Rust** 1.70 或更高版本
- **Git**
- **编辑器**（推荐 VS Code + rust-analyzer）

### 克隆仓库

```bash
git clone https://github.com/yourusername/anytls-rs.git
cd anytls-rs
```

### 安装依赖

```bash
# Rust 工具链
rustup update stable

# 格式化工具
rustup component add rustfmt

# Linter
rustup component add clippy
```

### 构建项目

```bash
# 开发构建
cargo build

# 发布构建
cargo build --release
```

### 运行测试

```bash
# 所有测试
cargo test

# 单元测试
cargo test --lib

# 集成测试
cargo test --test '*'

# 完整自动化测试
powershell -ExecutionPolicy Bypass -File .\run_comprehensive_tests.ps1
```

---

## 开发流程

### 1. Fork 仓库

点击 GitHub 页面右上角的 "Fork" 按钮。

### 2. 创建分支

```bash
# 创建功能分支
git checkout -b feature/your-feature-name

# 或者 bug 修复分支
git checkout -b fix/bug-description
```

**分支命名规范**:

- `feature/` - 新功能
- `fix/` - Bug 修复
- `docs/` - 文档更新
- `refactor/` - 代码重构
- `test/` - 测试相关
- `chore/` - 杂项更改

### 3. 进行更改

- 遵循代码规范（见下文）
- 编写测试
- 更新文档
- 频繁提交

### 4. 运行检查

```bash
# 格式化
cargo fmt

# Clippy 检查
cargo clippy --all-targets -- -D warnings

# 运行测试
cargo test
```

### 5. 提交更改

```bash
git add .
git commit -m "feat: add amazing feature"
```

遵循[提交规范](#提交规范)。

### 6. 推送到 Fork

```bash
git push origin feature/your-feature-name
```

### 7. 创建 Pull Request

在 GitHub 上创建 Pull Request，遵循 [PR 流程](#pull-request-流程)。

---

## 代码规范

### Rust 风格指南

遵循 [Rust 官方风格指南](https://doc.rust-lang.org/nightly/style-guide/)。

### 关键规则

#### 1. 格式化

```bash
# 自动格式化
cargo fmt
```

#### 2. Naming

```rust
// 类型使用 PascalCase
struct MyStruct;
enum MyEnum;

// 函数和变量使用 snake_case
fn my_function() {}
let my_variable = 10;

// 常量使用 SCREAMING_SNAKE_CASE
const MAX_CONNECTIONS: usize = 100;
```

#### 3. 注释

```rust
/// 函数的文档注释
///
/// # Examples
///
/// ```
/// let result = my_function(42);
/// assert_eq!(result, 84);
/// ```
pub fn my_function(value: i32) -> i32 {
    value * 2
}

// 内联注释解释复杂逻辑
let result = complex_calculation(); // 计算结果
```

#### 4. 错误处理

```rust
// 使用 Result 而不是 panic
pub fn risky_operation() -> Result<String, AnyTlsError> {
    if condition {
        Ok("success".to_string())
    } else {
        Err(AnyTlsError::SomeError)
    }
}

// 使用 ? 操作符
let value = risky_operation()?;
```

#### 5. 异步代码

```rust
// 异步函数清晰标注
pub async fn async_operation() -> Result<()> {
    // 使用 .await
    let result = future.await?;
    Ok(())
}
```

### Clippy

确保没有 Clippy 警告：

```bash
cargo clippy --all-targets -- -D warnings
```

常见 Clippy 修复：

```rust
// 避免不必要的 clone
❌ let s = string.clone();
✅ let s = string; // 如果不再使用 string

// 使用 if let
❌ match option {
    Some(x) => do_something(x),
    None => {}
}
✅ if let Some(x) = option {
    do_something(x);
}

// 避免不必要的 unwrap
❌ let value = option.unwrap();
✅ let value = option.expect("should have value");
```

---

## 测试要求

### 测试类型

#### 1. 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let result = my_function(42);
        assert_eq!(result, 84);
    }

    #[tokio::test]
    async fn test_async_functionality() {
        let result = async_function().await.unwrap();
        assert_eq!(result, "expected");
    }
}
```

#### 2. 集成测试

在 `tests/` 目录下：

```rust
// tests/my_integration_test.rs
use anytls_rs::*;

#[tokio::test]
async fn test_end_to_end() {
    // 测试完整流程
}
```

#### 3. 文档测试

```rust
/// 计算两个数的和
///
/// # Examples
///
/// ```
/// use anytls_rs::add;
/// assert_eq!(add(2, 2), 4);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### 测试覆盖要求

- **新功能**: 必须有测试
- **Bug 修复**: 必须有回归测试
- **核心模块**: 覆盖率 > 80%

### 运行测试

```bash
# 所有测试
cargo test

# 特定模块
cargo test session::

# 显示输出
cargo test -- --nocapture

# 单线程运行
cargo test -- --test-threads=1
```

---

## 文档要求

### 代码文档

#### Public API

所有 public 函数、结构、trait 必须有文档：

```rust
/// Session 管理多个 Stream 的复用连接
///
/// Session 使用单个 TLS 连接承载多个逻辑 Stream，
/// 实现了高效的连接复用。
///
/// # Examples
///
/// ```no_run
/// use anytls_rs::Session;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let session = Session::new(reader, writer, true);
/// let stream = session.open_stream().await?;
/// # Ok(())
/// # }
/// ```
pub struct Session {
    // ...
}
```

#### Private 函数

复杂的 private 函数也应有注释：

```rust
// 处理接收到的 Frame 并分发到对应 Stream
//
// 根据 Frame 的 command 类型和 stream_id，
// 将数据发送到对应的 Stream 或执行控制操作。
fn handle_frame(&self, frame: Frame) -> Result<()> {
    // ...
}
```

### Markdown 文档

#### 更新现有文档

- 保持一致性
- 更新示例
- 修正过时信息

#### 创建新文档

- 清晰的标题
- 目录（如果内容较长）
- 代码示例
- 截图（如果有帮助）

### 文档测试

```bash
# 测试文档中的示例
cargo test --doc
```

---

## 提交规范

### Conventional Commits

使用 [Conventional Commits](https://www.conventionalcommits.org/) 规范。

### 格式

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Type

- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `style`: 代码格式（不影响功能）
- `refactor`: 重构
- `perf`: 性能优化
- `test`: 测试相关
- `chore`: 构建/工具相关

### Examples

```bash
# 新功能
git commit -m "feat(session): add stream multiplexing support"

# Bug 修复
git commit -m "fix(stream): resolve second request blocking issue"

# 文档
git commit -m "docs: update README with v0.2.0 changes"

# 重构
git commit -m "refactor(stream): separate reader and writer"

# 测试
git commit -m "test: add comprehensive integration tests"
```

### 多行提交

```bash
git commit -m "feat(session): add stream multiplexing

This commit adds support for multiplexing multiple streams
over a single TLS connection.

Changes:
- Add Stream struct
- Implement frame routing
- Add stream lifecycle management

Closes #123"
```

---

## Pull Request 流程

### 创建 PR

1. **标题**: 清晰描述更改
   ```
   feat: Add HTTP proxy support
   fix: Resolve memory leak in Stream
   docs: Update architecture documentation
   ```

2. **描述**: 使用模板

```markdown
## 描述

简要描述这个 PR 的目的。

## 更改类型

- [ ] Bug 修复
- [ ] 新功能
- [ ] 重构
- [ ] 文档更新
- [ ] 性能优化
- [ ] 测试

## 更改内容

- 添加了 X 功能
- 修复了 Y 问题
- 重构了 Z 模块

## 相关 Issue

Closes #123
Related to #456

## 测试

- [ ] 单元测试通过
- [ ] 集成测试通过
- [ ] 手动测试完成

## 检查清单

- [ ] 代码遵循项目风格
- [ ] 已添加测试
- [ ] 所有测试通过
- [ ] 已更新文档
- [ ] Clippy 无警告
- [ ] 代码已格式化

## 截图（如果适用）

```

### PR 审查

维护者会审查您的 PR，可能会：

- 请求更改
- 提出建议
- 合并 PR

### 审查后

如果需要更改：

```bash
# 进行更改
git add .
git commit -m "fix: address review comments"
git push origin feature/your-feature
```

PR 会自动更新。

---

## 发布流程

（仅限维护者）

### 1. 更新版本

```bash
# 更新 Cargo.toml
version = "0.3.0"

# 更新 CHANGELOG.md
## [0.3.0] - 2025-XX-XX
```

### 2. 创建标签

```bash
git tag -a v0.3.0 -m "Version 0.3.0: Description"
git push origin v0.3.0
```

### 3. 发布到 crates.io

```bash
cargo publish
```

---

## 获取帮助

### 文档

- [README.md](README.md) - 项目概览
- [ARCHITECTURE.md](ARCHITECTURE.md) - 架构文档
- [TEST_GUIDE.md](TEST_GUIDE.md) - 测试指南

### 社区

- [GitHub Discussions](https://github.com/yourusername/anytls-rs/discussions)
- [GitHub Issues](https://github.com/yourusername/anytls-rs/issues)

### 联系

- Email: your-email@example.com

---

## 许可证

贡献即表示您同意您的代码将以 MIT 许可证发布。

---

**感谢您的贡献！** 🎉

---

*最后更新: 2025-11-03*

