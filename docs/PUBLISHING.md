# 发布到 crates.io 指南

**最后更新**: 2025-11-04

---

## 📋 发布前检查清单

### 1. Cargo.toml 配置

确保 `Cargo.toml` 包含以下必需字段：

- [x] `name`: 包名（已设置：`anytls-rs`）
- [x] `version`: 版本号（当前：`0.3.0`）
- [x] `description`: 描述（已设置）
- [x] `license`: 许可证（已设置：`MIT`）
- [x] `repository`: 仓库地址（需要更新为实际地址）
- [x] `homepage`: 主页（需要更新为实际地址）
- [x] `documentation`: 文档地址（已设置：`https://docs.rs/anytls-rs`）
- [x] `readme`: README 文件（已设置：`README.md`）
- [x] `keywords`: 关键词（已设置）
- [x] `categories`: 分类（已设置）

### 2. 代码质量检查

```bash
# 格式化检查
cargo fmt --check

# Clippy 检查
cargo clippy --all-targets --all-features -- -D warnings

# 编译检查
cargo build --release

# 测试检查
cargo test --all-features
```

### 3. 文档检查

```bash
# 生成文档
cargo doc --no-deps --all-features

# 检查文档链接
cargo doc --no-deps --all-features --open
```

### 4. 版本号更新

在发布新版本前，更新 `Cargo.toml` 中的版本号：

```toml
[package]
version = "0.3.0"  # 更新为新版本
```

### 5. CHANGELOG 更新

更新 `docs/CHANGELOG.md`，添加新版本的变更记录。

---

## 🚀 发布流程

### 方法 1: 使用 GitHub Actions（推荐）

#### 步骤 1: 配置 GitHub Secrets

1. 前往 GitHub 仓库设置：`Settings -> Secrets and variables -> Actions`
2. 添加 `CARGO_REGISTRY_TOKEN` secret：
   - 前往 https://crates.io/settings/tokens
   - 创建新的 API token（需要 `publish` 权限）
   - 复制 token 并添加到 GitHub Secrets

#### 步骤 2: 创建 Git 标签

```bash
# 确保版本号已更新
git add Cargo.toml
git commit -m "chore: bump version to 0.3.0"

# 创建并推送标签
git tag v0.3.0
git push origin v0.3.0
```

#### 步骤 3: 触发发布

GitHub Actions 会自动：
1. 运行测试
2. 构建发布版本
3. 发布到 crates.io
4. 创建 GitHub Release

### 方法 2: 手动发布

#### 步骤 1: 获取 crates.io token

1. 前往 https://crates.io/settings/tokens
2. 创建新的 API token（需要 `publish` 权限）
3. 保存 token

#### 步骤 2: 登录 crates.io

```bash
cargo login <your-token>
```

#### 步骤 3: 验证包

```bash
# 检查包元数据
cargo package --list

# 验证包
cargo package --verify
```

#### 步骤 4: 发布

```bash
# 发布到 crates.io
cargo publish
```

---

## 📝 发布检查清单

### 发布前

- [ ] 更新 `Cargo.toml` 版本号
- [ ] 更新 `CHANGELOG.md`
- [ ] 运行所有测试：`cargo test`
- [ ] 运行 Clippy：`cargo clippy --all-targets --all-features -- -D warnings`
- [ ] 检查格式化：`cargo fmt --check`
- [ ] 验证包：`cargo package --verify`
- [ ] 检查文档：`cargo doc --no-deps --all-features`
- [ ] 更新 `Cargo.toml` 中的 `repository` 和 `homepage` URL

### 发布时

- [ ] 创建 Git 标签：`git tag v0.3.0`
- [ ] 推送标签：`git push origin v0.3.0`
- [ ] 等待 GitHub Actions 完成发布
- [ ] 或手动运行：`cargo publish`

### 发布后

- [ ] 验证包已发布：https://crates.io/crates/anytls-rs
- [ ] 检查文档：https://docs.rs/anytls-rs
- [ ] 创建 GitHub Release（如果使用 GitHub Actions，会自动创建）
- [ ] 更新 README.md 中的版本号（如果需要）

---

## 🔧 CI/CD 配置

### GitHub Actions Workflows

已创建以下 workflows：

1. **`.github/workflows/ci.yml`**
   - 测试、构建、Clippy 检查
   - 在 push 和 PR 时运行

2. **`.github/workflows/benchmark.yml`**
   - 运行基准测试
   - 检测性能回归

3. **`.github/workflows/publish.yml`**
   - 发布到 crates.io
   - 在创建 Release 时触发

4. **`.github/workflows/release.yml`**
   - 构建发布版本
   - 创建 GitHub Release
   - 发布到 crates.io

### 配置 Secrets

在 GitHub 仓库设置中添加：

- `CARGO_REGISTRY_TOKEN`: crates.io API token

---

## 📊 版本号规则

遵循 [语义化版本](https://semver.org/)：

- **主版本号** (MAJOR): 不兼容的 API 变更
- **次版本号** (MINOR): 向后兼容的功能添加
- **修订号** (PATCH): 向后兼容的问题修复

当前版本：`0.3.0`

---

## 🐛 常见问题

### 1. 发布失败：包名已存在

如果包名 `anytls-rs` 已被占用，需要：
- 更改包名（需要更新所有引用）
- 或联系 crates.io 管理员

### 2. 发布失败：版本已存在

如果版本已发布，需要：
- 更新版本号
- 重新发布

### 3. 发布失败：缺少必需字段

检查 `Cargo.toml` 是否包含所有必需字段：
- `description`
- `license`
- `repository`（推荐）
- `homepage`（推荐）
- `documentation`（推荐）

### 4. GitHub Actions 失败

检查：
- Secrets 是否正确配置
- 版本号是否匹配
- 测试是否通过

---

## 📚 相关资源

- [crates.io 发布指南](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [语义化版本](https://semver.org/)
- [GitHub Actions 文档](https://docs.github.com/en/actions)

---

## 🔗 相关文档

- [CI/CD 配置说明](../.github/workflows/)
- [变更日志](CHANGELOG.md)
- [发布说明](RELEASE_NOTES_v0.3.0.md)

