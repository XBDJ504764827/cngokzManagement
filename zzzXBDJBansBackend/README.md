# zzzXBDJBans Backend API (Rust)

![Rust](https://img.shields.io/badge/Rust-1.80%2B-000000?style=for-the-badge&logo=rust&logoColor=white)
![Axum](https://img.shields.io/badge/Axum-0.7-FF5722?style=for-the-badge&logo=rust&logoColor=white)
![SQLx](https://img.shields.io/badge/SQLx-0.8-336791?style=for-the-badge&logo=postgresql&logoColor=white)

zzzXBDJBans 的核心后端服务，使用 Rust 编写，基于 Axum 框架构建。为前端管理界面和 CSGO 插件提供高性能的 RESTful API 支持，处理封禁、验证和数据存储。

## ✨ 技术栈

- **Web 框架**: Axum (基于 Tokio)
- **数据库 ORM**: SQLx (异步、类型安全)
- **数据库**: PostgreSQL
- **缓存**: Redis (用于会话管理和临时数据)
- **文档**: Utoipa (Swagger UI)

## 🛠️ 环境要求

- **Rust**: 推荐使用最新 Stable 版本 (`rustup update`)
- **PostgreSQL**: >= 14
- **Redis**: >= 6.0
- **SQLx CLI**:用于数据库迁移 (`cargo install sqlx-cli`)

## 🚀 快速开始

### 1. 配置数据库

首先创建数据库 `zzzXBDJBans`。

项目包含数据库迁移脚本，位于 `migrations` 目录。请使用 SQLx CLI 运行迁移：

```bash
# 设置数据库连接 URL (替换为您的实际配置)
export DATABASE_URL="postgres://user:password@localhost:5432/zzzXBDJBans"

# 运行迁移
sqlx migrate run
```

### 2. 配置环境变量

复制 `.env.example` 为 `.env` 并根据环境修改：

```bash
cp .env.example .env
```

**配置项示例**:

```ini
DATABASE_URL=postgres://root:password@localhost:5432/zzzXBDJBans
REDIS_URL=redis://127.0.0.1:6379/
RUST_LOG=info,zzzXBDJBansBackend=debug
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
STEAM_API_KEY=your_steam_web_api_key
```

### 3. 构建与运行

开发模式运行（支持热重载需安装 `cargo-watch`）：

```bash
cargo run
```

或者构建发布版本：

```bash
cargo build --release
./target/release/zzzXBDJBansBackend
```

## 📚 API 文档

后端启动后，访问 `/swagger-ui/` 即可查看完整的 Swagger API 文档和测试接口。

例如：`http://localhost:8080/swagger-ui/`

## 📂 目录结构

```
zzzXBDJBansBackend/
├── src/
│   ├── handlers/      # API 路由处理函数
│   ├── models/        # 数据模型 (Structs)
│   ├── services/      # 业务逻辑层
│   ├── db/            # 数据库连接与操作
│   ├── main.rs        # 程序入口
│   └── lib.rs         # 库入口
├── migrations/        # SQLx 数据库迁移文件
├── Cargo.toml         # Rust 依赖配置
└── .env               # 环境配置
```

## 🤝 贡献

欢迎提交 Pull Request 或 Issue 来改进本项目。

## 📄 许可证

[MIT License](LICENSE)
