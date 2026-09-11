# TAP App Attest Server

[English](README.md) | 简体中文

## 用途

为 TAPCam 提供 App Attest 凭证注册和拍摄签名验证的 Rust 服务。它签发 challenge、
验证 Apple attestation、在 Redis 中保存凭证，并验证处于 active 状态的已注册
密钥是否签署了拍摄产物的 `signingBinding`。

[TAPArtifactContracts](https://github.com/TAP-NAP/TAPArtifactContracts)是产品、
产物和后端要求的规范来源。本仓库说明服务实现与运行方式；协议阅读入口是
[BackendContract](https://github.com/TAP-NAP/TAPArtifactContracts/blob/main/BackendContract.md)
和共享的
[后端 App Attest 验证规则](https://github.com/TAP-NAP/TAPArtifactContracts/blob/main/bindings/capture-binding-and-proof-v1.md#backend-app-attest-gate)。

## 使用方法

本地开发需要 Rust 和 Redis。先复制并修改示例配置：

```sh
cp .env.example .env
# Edit TEAM_ID, BUNDLE_ID and APP_ATTEST_ENV to match the signing app.
redis-server --appendonly yes
```

在另一个终端中进入仓库根目录并运行：

```sh
set -a
source .env
set +a
cargo run --locked
```

程序读取进程环境变量，不会自动加载 `.env`。示例配置监听
`127.0.0.1:8080`。运行单元测试或构建容器镜像：

```sh
cargo test --locked
docker build -t tap-app-attest-server:latest .
```

Linux 主机使用[部署控制台](deploy/README.md)，由操作者手动拉取预构建的后端
镜像和网站。主机配置是 `/etc/tapnap.conf`，与本地 `.env` 分开。

`GET /healthz` 检查服务和 Redis 是否可用。已实现的 App Attest 路由见
[src/routes.rs](src/routes.rs)，请求字段和响应含义由上述服务端契约定义。
拍摄验证接口的跨域策略允许来源为 `https://verifier.tapnap.net`。

## 简要原理

注册流程是 `challenge -> Apple attestation 验证 -> Redis 凭证`。服务检查
配置的应用身份和环境，原子消费 challenge，并使用本地 Apple 根证书。
Docker 镜像在 `/app/certs/` 下自带该证书。

拍摄验证流程是 `signingBinding -> canonical JSON -> 已注册公钥 + App Attest
assertion -> valid/invalid`。调用方先完成本地产物绑定验证，再提交共享签名
请求。本服务不接收原始媒体，也不重算其 content digest；最终产物结论必须结合
调用方已通过的本地结果。

当前实现使用配置的 App ID 验证 assertion，并要求存储的凭证处于 active 状态。
拍摄验证使用 `AssertionCounterPolicy::Unchecked`，允许离线产物乱序提交；
此路径没有 freshness challenge、counter 更新或 capture-ID 去重。Challenge
接口接受 `assertion` 用途，并不代表服务已实现通用业务请求验证接口。账号授权
及长期事件审计也不属于当前 API 的已实现能力。

Redis 凭证记录不设 TTL；字段与备份语义见
[Redis 数据结构](docs/REDIS_SCHEMA.md)。逐请求业务日志默认关闭；需要时设置
`REQUEST_LOGS` 和 `RUST_LOG`。日志不输出原始 challenge 或 attestation/assertion
载荷。

## 目录结构

| 路径 | 职责 |
| --- | --- |
| `src/main.rs`, `src/config.rs` | 启动、环境配置与退出。 |
| `src/routes.rs`, `src/models.rs`, `src/error.rs` | HTTP 接口、JSON 模型与错误响应。 |
| `src/challenge.rs`, `src/verifier.rs` | Challenge 基础操作与 App Attest 验证调用。 |
| `src/store/` | Redis 记录及 challenge 原子消费。 |
| `certs/` | 镜像内置的 Apple App Attest 根证书。 |
| `deploy/` | 主机控制台和运行说明。 |
| `docs/REDIS_SCHEMA.md` | 持久化数据格式与生命周期。 |
| `Cargo.toml`, `Cargo.lock`, `Dockerfile`, `.env.example` | 构建依赖、镜像及本地配置。 |

单元测试与 Rust 实现放在 `src/` 内。

## 仓库依赖

| 仓库 | 关系 |
| --- | --- |
| [TAPArtifactContracts](https://github.com/TAP-NAP/TAPArtifactContracts) | 产品、产物和 HTTP 要求的规范来源；文档依赖，不是可执行代码。 |
| [attestation_assertion_verifier](https://github.com/TAP-NAP/attestation_assertion_verifier) | 提供 `apple_app_attest_attestation` 的 Rust Git 依赖；`Cargo.lock` 固定解析后的版本。 |
| [TAPCamDemo](https://github.com/TAP-NAP/TAPCamDemo) | 原生拍摄端，由相机应用调用注册和凭证状态 HTTP 接口。 |
| [TAPCamVerifier](https://github.com/TAP-NAP/TAPCamVerifier) | 浏览器拍摄验证客户端；其生成的 `ecs-web` 分支同时为 `tap` 提供待部署的静态网站。 |

两个客户端都不是 Rust 构建依赖。其他 Rust 包列在 `Cargo.toml`；Redis 是运行时
必需服务。Linux 主机工具见[部署指南](deploy/README.md)。网站是部署输入，不定义
本服务的协议。
