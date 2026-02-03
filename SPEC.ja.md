# SPEC.ja.md --- Rust マイクロサービス on Kubernetes（教育プラットフォーム）

## 0. 目標

Kubernetes 上で動作する、Rust による最小限かつ現実的なマイクロサービスシステムを構築する。ドメイン: 以下の 3 サービスからなる教育プラットフォーム。
- admin-service（管理者サービス）
- teacher-service（教師サービス）
- student-service（学生サービス）

主目的: Kubernetes ネイティブなサービスディスカバリ（DNS）、サービスごとのデータ所有、および非同期 Rust Web API の習得。

## 1. スコープ（MVP）

### 必須

- Kubernetes にデプロイする 3 つの独立した HTTP サービス（Rust、非同期）
- Kubernetes ネイティブなサービスディスカバリ（Eureka は使わない）
- 各サービスが自身のスキーマを所有（1 DB、スキーマ: admin, teacher, student）
- 垂直スライス:
  1) 管理者が Course（コース）を作成
  2) 教師が Assignment（課題）を作成
  3) 学生が Submission（提出物）を提出
- ロール（admin, teacher, student）による JWT 検証
- ヘルス / レディネスエンドポイント
- kind / minikube で動作する K8s マニフェスト

## 2. 技術選定

- axum, tokio, reqwest
- sqlx + PostgreSQL
- config + 環境変数
- tracing

## 3. アーキテクチャ

- サービス間は同期 REST
- DNS 名:
  - http://admin-service:8080
  - http://teacher-service:8080
  - http://student-service:8080

## 4. API（MVP）

**Admin（管理者）**
- POST /api/admin/courses
- GET /api/admin/courses/{course_id}

**Teacher（教師）**
- POST /api/teacher/courses/{course_id}/assignments
- GET /api/teacher/assignments/{assignment_id}

**Student（学生）**
- POST /api/student/assignments/{assignment_id}/submissions

## 5. データモデル

- admin.courses
- teacher.assignments
- student.submissions

## 6. Kubernetes

- Namespace: edu
- Postgres 1 台、1 データベース、スキーマ 3 つ（admin, teacher, student）
- サービスごとに Deployment + Service + ConfigMap + Secret
- Ingress（または Gateway）を単一エントリポイントとし、/api/admin/*、/api/teacher/*、/api/student/* を各サービスにルーティング

## 7. クロスカッティング・レジリエンス（必須で実装）

- **Service Discovery（K8s DNS）**  
  サービス間の呼び出しは DNS のみ: `http://<service-name>:8080`（同一 namespace）。Eureka 等の外部ディスカバリは使わない。

- **Config（ConfigMap / Secret）**  
  ConfigMap: 非機密（SERVICE_NAME, HTTP_PORT, RUST_LOG, DB ホスト）。Secret: DATABASE_URL, JWT_SECRET。環境変数で注入。

- **Auth（JWT / OIDC）**  
  保護対象 API では必ず JWT 検証: 署名検証（JWT_SECRET）、exp/iss 検証、ロール（admin/teacher/student）取得。MVP は共有シークレット。将来 OIDC 発行元（例: Keycloak）対応も可。

- **Gateway / Ingress**  
  前面に 1 つの Ingress（または API Gateway）を置き、パスベースで admin-service / teacher-service / student-service に振り分け。TLS は MVP では任意。

- **Timeout**  
  外向き HTTP（reqwest）: connect_timeout と timeout を設定（例: 5s / 30s）。内向き（axum）: タイムアウトミドルウェアで遅いリクエストを打ち切り。

- **Retry**  
  外向き呼び出し: 指数バックオフでリトライ（例: 3 回、冪等または安全な操作のみ）。reqwest の retry や tower を利用。

- **Circuit Breaker**  
  他サービスへの外向き呼び出しにサーキットブレーカー（例: tower）を適用し、連続失敗でオープンして連鎖障害を防ぐ。ハーフオープンで復帰を試行。

- **Health Check**  
  Liveness: GET /health（または /live）— プロセス生存。Readiness: GET /ready — DB 接続済みでトラフィック受付可能。K8s の livenessProbe / readinessProbe をこれらのエンドポイントに設定。

## 8. 環境変数

- SERVICE_NAME
- HTTP_PORT
- DATABASE_URL
- JWT_SECRET
- RUST_LOG

## 9. 受入基準

- 全サービスがヘルシーであること（liveness/readiness が通ること）
- 垂直スライスが動作すること
- サービスが K8s DNS 経由で通信すること
- Config は ConfigMap/Secret から、認証は JWT、外向き呼び出しに timeout・retry・circuit breaker、Ingress でルーティングされていること
