# Education Platform — Rust Microservices on Kubernetes

Kubernetes 上で動作する Rust によるマイクロサービスアーキテクチャの実装例です。教育プラットフォームを題材に、3 つの独立したマイクロサービス（admin-service、teacher-service、student-service）で構成されています。

## 概要

このプロジェクトは、**実践的なマイクロサービスのパターン**を学ぶための実装例です。以下の特徴を持ちます：

- **Kubernetes ネイティブなサービスディスカバリ**（DNS ベース）
- **JWT 認証とロールベースアクセス制御**（RBAC）
- **レジリエンスパターン**（タイムアウト、リトライ、サーキットブレーカー）
- **ヘルスチェック**（Liveness / Readiness）
- **設定管理**（ConfigMap / Secret）
- **サービスごとのデータ所有**（スキーマ分離）

詳細な解説は [ブログ記事](./docs/BLOG-ja.md) を参照してください。

## アーキテクチャ

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   Ingress   │
└──────┬──────┘
       │
       ├──────────────┬──────────────┐
       ▼              ▼              ▼
┌──────────┐  ┌──────────┐  ┌──────────┐
│  admin-  │  │ teacher- │  │ student- │
│ service  │◄─│ service  │◄─│ service  │
└────┬─────┘  └──────────┘  └──────────┘
     │
     ▼
┌──────────┐
│ Postgres │
│ (3 schemas)│
└──────────┘
```

### サービス構成

- **admin-service**: コース管理（管理者向け）
- **teacher-service**: 課題管理（教師向け、admin-service を呼び出し）
- **student-service**: 提出物管理（学生向け、teacher-service を呼び出し）

### データベース

- PostgreSQL（1 データベース、3 スキーマ）
  - `admin.courses`
  - `teacher.assignments`
  - `student.submissions`

## 前提条件

- **Rust** (stable チャンネル、`rustup default stable`)
- **Docker**
- **minikube**（推奨）または **kind**
- **kubectl**

## クイックスタート

### 1. リポジトリのクローン

```bash
git clone <repository-url>
cd microservice-practice
```

### 2. ローカルセットアップ（minikube）

ワンコマンドで環境構築からデプロイまで実行します：

```bash
./scripts/run-local-setup.sh
```

このスクリプトは以下を自動実行します：
- minikube の起動
- Docker イメージのビルド（admin-service、teacher-service、student-service）
- Kubernetes マニフェストの適用（namespace、Postgres、マイグレーション、各サービス、Ingress）
- port-forward の実行方法を表示

### 3. port-forward の設定

別ターミナルで以下を実行：

```bash
kubectl port-forward -n edu svc/admin-service 8080:8080 &
kubectl port-forward -n edu svc/teacher-service 8081:8080 &
kubectl port-forward -n edu svc/student-service 8082:8080 &
```

### 4. 動作確認

**インテグレーションテスト**を実行して、3 サービスの連携を確認：

```bash
./scripts/integration-test.sh
```

または、手動で API を呼び出す場合は [QUICKSTART-ja.md](./docs/QUICKSTART-ja.md) の「動作確認（垂直スライス）」を参照してください。

## 開発ワークフロー

### コード変更の反映

サービスを変更した後、再ビルドとデプロイを自動化するスクリプト：

```bash
# 1 つのサービスだけ再デプロイ
./scripts/redeploy-service.sh admin-service

# 全サービスを再デプロイ
./scripts/redeploy-service.sh all
```

このスクリプトは以下を自動実行します：
- minikube の Docker でイメージをビルド
- Deployment を再起動（`kubectl rollout restart`）
- Pod が Ready になるまで待機
- port-forward を自動的に再起動

### テストトークンの生成

JWT トークンを生成するスクリプト：

```bash
SECRET="your-jwt-secret-change-in-production"
TOKEN_ADMIN=$(./scripts/gen-jwt.sh admin "$SECRET")
TOKEN_TEACHER=$(./scripts/gen-jwt.sh teacher "$SECRET")
TOKEN_STUDENT=$(./scripts/gen-jwt.sh student "$SECRET")
```

## プロジェクト構成

```
microservice-practice/
├── admin-service/          # 管理者サービス
│   ├── src/
│   │   ├── main.rs        # エントリーポイント
│   │   ├── lib.rs         # ルーティング設定
│   │   └── routes.rs      # API ハンドラ
│   └── Cargo.toml
├── teacher-service/        # 教師サービス
│   └── ...
├── student-service/        # 学生サービス
│   └── ...
├── shared/                 # 共有ライブラリ
│   └── src/
│       ├── auth.rs         # JWT 認証
│       ├── config.rs       # 設定管理
│       ├── http_client.rs  # HTTP クライアント（リトライ、サーキットブレーカー）
│       └── tracing_init.rs # ロギング設定
├── migrations/             # データベースマイグレーション
│   ├── 001_create_schemas.sql
│   ├── 002_admin_courses.sql
│   ├── 003_teacher_assignments.sql
│   └── 004_student_submissions.sql
├── k8s/                    # Kubernetes マニフェスト
│   ├── namespace.yaml
│   ├── postgres-*.yaml
│   ├── admin-service-*.yaml
│   ├── teacher-service-*.yaml
│   ├── student-service-*.yaml
│   └── ingress.yaml
├── scripts/                # 開発支援スクリプト
│   ├── run-local-setup.sh      # ローカル環境構築
│   ├── redeploy-service.sh     # サービス再デプロイ
│   ├── integration-test.sh     # インテグレーションテスト
│   └── gen-jwt.sh              # JWT トークン生成
└── docs/
    ├── QUICKSTART-ja.md    # 詳細な起動手順
    └── BLOG-ja.md          # マイクロサービスパターンの解説記事
```

## API エンドポイント

### Admin Service (port 8080)

| Method | Path                          | Role   | 説明           |
|--------|-------------------------------|--------|----------------|
| POST   | `/api/admin/courses`          | admin  | コース作成     |
| GET    | `/api/admin/courses/:id`      | admin  | コース取得     |
| GET    | `/health`                     | -      | ヘルスチェック |
| GET    | `/ready`                      | -      | レディネス     |

### Teacher Service (port 8081)

| Method | Path                                      | Role   | 説明           |
|--------|-------------------------------------------|--------|----------------|
| POST   | `/api/teacher/courses/:id/assignments`   | teacher| 課題作成       |
| GET    | `/api/teacher/assignments/:id`           | teacher| 課題取得       |
| GET    | `/health`                                 | -      | ヘルスチェック |
| GET    | `/ready`                                  | -      | レディネス     |

### Student Service (port 8082)

| Method | Path                                      | Role   | 説明           |
|--------|-------------------------------------------|--------|----------------|
| POST   | `/api/student/assignments/:id/submissions`| student| 提出物作成     |
| GET    | `/health`                                 | -      | ヘルスチェック |
| GET    | `/ready`                                  | -      | レディネス     |

**認証**: 保護されたエンドポイントには `Authorization: Bearer <JWT>` ヘッダが必要です。

## 主要な機能

### サービスディスカバリ

- Kubernetes DNS を使用（`http://admin-service:8080` など）
- 外部のサービスディスカバリ（Eureka、Consul など）は不要

### 認証・認可

- JWT ベースの認証
- ロールベースアクセス制御（admin、teacher、student）
- サービス間通信では JWT を転送してユーザーコンテキストを保持

### レジリエンスパターン

- **タイムアウト**: 接続タイムアウト 5 秒、リクエストタイムアウト 30 秒
- **リトライ**: 最大 3 回、指数バックオフ（100ms、200ms、400ms）
- **サーキットブレーカー**: 連続 5 回失敗でオープン、30 秒後にハーフオープン

### ヘルスチェック

- **Liveness Probe**: `/health` - プロセス生存確認
- **Readiness Probe**: `/ready` - DB 接続確認

### 設定管理

- **ConfigMap**: 非機密情報（サービス名、ポート、ログレベル）
- **Secret**: 機密情報（データベース URL、JWT シークレット）

詳細は [ブログ記事](./docs/BLOG-ja.md) の「設定管理」セクションを参照してください。

## 詳細ドキュメント

- **[QUICKSTART-ja.md](./docs/QUICKSTART-ja.md)**: 詳細な起動手順、トラブルシューティング
- **[BLOG-ja.md](./docs/BLOG-ja.md)**: マイクロサービスのパターンと実装の詳細解説
- **[SPEC.ja.md](./SPEC.ja.md)**: プロジェクトの仕様書

## トラブルシューティング

### Pod が起動しない

```bash
# Pod の状態を確認
kubectl get pods -n edu

# ログを確認
kubectl logs -n edu -l app=admin-service --tail=50
```

### サービス間通信が失敗する

```bash
# クラスタ内から admin-service にアクセスできるか確認
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -n edu -- \
  curl -s http://admin-service:8080/health
```

### port-forward が接続できない

```bash
# 既存の port-forward を kill
pkill -f "kubectl port-forward"

# 新しい port-forward を起動
kubectl port-forward -n edu svc/admin-service 8080:8080 &
kubectl port-forward -n edu svc/teacher-service 8081:8080 &
kubectl port-forward -n edu svc/student-service 8082:8080 &
```

詳細なトラブルシューティングは [QUICKSTART-ja.md](./docs/QUICKSTART-ja.md) を参照してください。

## ライセンス

このプロジェクトは学習目的の実装例です。

## 参考リソース

- [Kubernetes Documentation](https://kubernetes.io/docs/)
- [Axum Web Framework](https://github.com/tokio-rs/axum)
- [Rust Book](https://doc.rust-lang.org/book/)
