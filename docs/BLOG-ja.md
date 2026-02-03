# Kubernetes 上で Rust マイクロサービスを実装して学んだ、実践的なマイクロサービスパターン

## はじめに

マイクロサービスアーキテクチャを学ぶ上で、理論だけでなく実際に手を動かして構築することが重要です。本記事では、Kubernetes 上で動作する教育プラットフォーム（3 つのマイクロサービス）を Rust で実装した経験から、**実践的なマイクロサービスのパターンとベストプラクティス**を紹介します。

Rust のコード詳細よりも、**マイクロサービスアーキテクチャの設計思想と実装パターン**に焦点を当てて解説します。

## プロジェクト概要

実装したシステムは、以下の 3 つのマイクロサービスで構成されています：

- **admin-service**: コース管理（管理者向け）
- **teacher-service**: 課題管理（教師向け）
- **student-service**: 提出物管理（学生向け）

各サービスは独立してデプロイ可能で、Kubernetes 上で動作します。垂直スライス（エンドツーエンドのユーザーシナリオ）は以下の通りです：

1. 管理者がコースを作成（admin-service）
2. 教師が課題を作成（teacher-service → admin-service を呼んでコース存在確認）
3. 学生が提出（student-service → teacher-service を呼んで課題存在確認）

## マイクロサービスの特徴と実装パターン

### 1. Kubernetes ネイティブなサービスディスカバリ

**特徴**: 外部のサービスディスカバリ（Eureka、Consul など）を使わず、Kubernetes の DNS を活用

**実装**:
- サービス間の通信は `http://<service-name>:8080` という DNS 名で行う
- 同一 namespace 内では、Kubernetes が自動的に DNS レコードを提供
- 例: `http://admin-service:8080`, `http://teacher-service:8080`

**メリット**:
- **シンプル**: 追加のインフラコンポーネントが不要
- **標準的**: Kubernetes の標準機能を活用
- **運用が楽**: DNS の管理が Kubernetes に委ねられる

**注意点**:
- サービス名と namespace を正確に指定する必要がある
- ローカル開発時は port-forward や Ingress で外部からアクセス可能にする

### 2. サービスごとのデータ所有（Database per Service の変形）

**特徴**: 各サービスが自身のスキーマを所有する

**実装**:
- 1 つの PostgreSQL データベースを使用
- スキーマを分離: `admin`, `teacher`, `student`
- 各サービスは自分のスキーマのみにアクセス

```
admin.courses          # admin-service のみアクセス
teacher.assignments    # teacher-service のみアクセス
student.submissions    # student-service のみアクセス
```

**メリット**:
- **独立性**: スキーマ変更が他サービスに影響しない
- **段階的移行**: 完全な Database per Service への移行が容易
- **コスト効率**: MVP 段階では 1 DB で運用可能

**将来の拡張**:
- 必要に応じて各サービスを独立したデータベースに分離可能
- スキーマ境界が明確なので、移行が比較的容易

### 3. JWT 認証とロールベースアクセス制御（RBAC）

**特徴**: 各サービスが JWT を検証し、ロールに基づいてアクセス制御

**実装の全体像**:

このプロジェクトでは、JWT 認証を **分散認証** の形で実装しています。各サービスが独立して JWT を検証し、ロールに基づいてアクセス制御を行います。

#### JWT の構造と検証

**Claims 構造体**:

```rust
#[derive(Debug, Deserialize)]
pub struct Claims {
    pub sub: String,      // ユーザーID
    pub role: Role,       // ロール（admin/teacher/student）
    pub exp: i64,         // 有効期限（Unix timestamp）
    pub iss: Option<String>, // 発行者（オプション）
}
```

**Role enum**:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Teacher,
    Student,
}
```

JWT のペイロードには、ユーザーID（`sub`）とロール（`role`）が含まれます。例: `{"sub": "alice", "role": "teacher", "exp": 1738299999}`

**JWT 検証の実装** (`verify_jwt` 関数):

```rust
pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims, Error> {
    let key = DecodingKey::from_secret(secret.as_ref());
    let mut validation = Validation::default();
    validation.validate_exp = true;  // 有効期限を検証
    let data = decode::<Claims>(token, &key, &validation)?;
    Ok(data.claims)
}
```

この関数は以下を行います：
1. **署名検証**: `JWT_SECRET` を使って、トークンが正しい秘密鍵で署名されているか確認
2. **有効期限検証**: `exp` フィールドをチェックし、期限切れのトークンを拒否
3. **Claims の抽出**: 検証が成功したら、ペイロードから `Claims` 構造体を取得

#### Axum での認証エクストラクタ

**AuthUser エクストラクタ** (`FromRequestParts` の実装):

各ルートハンドラで認証を自動化するため、Axum の `FromRequestParts` トレイトを実装しています：

```rust
pub struct AuthUser(pub Claims);

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // 1. Authorization ヘッダを取得
        let auth = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or((StatusCode::UNAUTHORIZED, "Missing Authorization header"))?;
        
        // 2. "Bearer " プレフィックスを除去
        let token = auth
            .strip_prefix("Bearer ")
            .ok_or((StatusCode::UNAUTHORIZED, "Invalid Authorization format"))?;
        
        // 3. 環境変数から JWT_SECRET を取得
        let secret = std::env::var("JWT_SECRET")
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "JWT_SECRET not set"))?;
        
        // 4. JWT を検証して Claims を取得
        let claims = verify_jwt(token, &secret)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid or expired token"))?;
        
        Ok(AuthUser(claims))
    }
}
```

**使用方法**:

ルートハンドラのパラメータに `AuthUser(auth)` を指定するだけで、自動的に認証が行われます：

```rust
pub async fn create_assignment(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,  // ← ここで自動的に認証・Claims 取得
    Path(course_id): Path<Uuid>,
    Json(body): Json<CreateAssignmentBody>,
) -> Result<...> {
    // auth.role と auth.sub が利用可能
    if auth.role != Role::Teacher {
        return Err((StatusCode::FORBIDDEN, "teacher role required"));
    }
    // ...
}
```

#### ロールベースアクセス制御（RBAC）の実装

各ルートハンドラで、`auth.role` をチェックしてアクセス制御を行います：

**例: teacher-service の課題作成**:

```rust
pub async fn create_assignment(
    AuthUser(auth): AuthUser,
    // ...
) -> Result<...> {
    // ロールチェック: Teacher のみ許可
    if auth.role != Role::Teacher {
        return Err((StatusCode::FORBIDDEN, "teacher role required"));
    }
    // Teacher ロールの場合のみ、ここ以降の処理が実行される
    // ...
}
```

**例: admin-service のコース取得（複数ロール許可）**:

```rust
pub async fn get_course(
    AuthUser(auth): AuthUser,
    Path(course_id): Path<Uuid>,
) -> Result<Json<Course>, ...> {
    // Admin と Teacher の両方を許可（Teacher はコース存在確認のため）
    if auth.role != Role::Admin && auth.role != Role::Teacher {
        return Err((StatusCode::FORBIDDEN, "admin or teacher role required"));
    }
    // ...
}
```

このように、**ルートごとに必要なロールを柔軟に設定**できます。

#### サービス間通信での JWT 転送

**問題**: teacher-service が admin-service を呼ぶ際、誰が呼び出しているのかを伝える必要がある

**解決策**: 元のリクエストの JWT をそのまま転送する

**実装の流れ**:

1. **teacher-service がリクエストを受信**:

```rust
pub async fn create_assignment(
    State(state): State<AppState>,
    headers: HeaderMap,  // ← Authorization ヘッダを取得
    AuthUser(auth): AuthUser,
    Path(course_id): Path<Uuid>,
    Json(body): Json<CreateAssignmentBody>,
) -> Result<...> {
    // ロールチェック
    if auth.role != Role::Teacher {
        return Err((StatusCode::FORBIDDEN, "teacher role required"));
    }
    
    // Authorization ヘッダから Bearer トークンを取得
    let bearer = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    
    // admin-service を呼び出す際に、JWT を転送
    let path = format!("/api/admin/courses/{}", course_id);
    let res = state.http_client.get_admin(&path, bearer).await?;
    // ...
}
```

2. **HTTP クライアントが JWT を転送**:

```rust
async fn request_with_retry(
    &self,
    url: &str,
    bearer_token: Option<&str>,  // ← 転送される JWT
) -> Result<reqwest::Response, HttpClientError> {
    // ...
    let mut req = self.client.get(url);
    if let Some(t) = bearer_token {
        req = req.header("Authorization", t);  // ← Authorization ヘッダを設定
    }
    // ...
}
```

3. **admin-service が転送された JWT を検証**:

admin-service は、転送された JWT を通常のリクエストと同様に検証します。`AuthUser` エクストラクタが自動的に処理するため、追加のコードは不要です。

**重要なポイント**:
- teacher-service は、**元のリクエストの JWT をそのまま転送**するだけ
- admin-service は、転送された JWT の `role` フィールドを確認し、`Teacher` ロールを許可（コース存在確認のため）
- これにより、**ユーザーコンテキストがサービス間で保持**される

#### メリットと注意点

**メリット**:
- **分散認証**: 各サービスが独立して認証を検証できるため、認証サーバーへの依存が少ない
- **トークン転送**: サービス間でユーザーコンテキストを保持し、誰が操作しているかを追跡可能
- **スケーラブル**: 署名検証のみなので、認証サーバーへの問い合わせが不要
- **型安全**: Rust の型システムにより、ロールの誤用をコンパイル時に検出可能

**注意点**:
- **JWT_SECRET の共有**: 全サービスで同じ `JWT_SECRET` を使用する必要がある（Kubernetes Secret で管理）
- **有効期限**: トークンの有効期限（`exp`）を適切に設定する必要がある
- **ロールの拡張**: 新しいロールを追加する場合は、すべてのサービスで `Role` enum を更新する必要がある（共有ライブラリ `shared/` を使用することで一元管理）
- **本番環境**: 専用の認証サービス（Auth0、Keycloak、Cognito など）でトークンを発行することを推奨。このプロジェクトでは検証のみを実装し、発行は別システムに委ねる設計

### 4. レジリエンスパターン: タイムアウト、リトライ、サーキットブレーカー

**特徴**: サービス間通信の信頼性を高めるための標準パターン

#### タイムアウト

**実装**:
- 接続タイムアウト: 5 秒
- リクエストタイムアウト: 30 秒
- HTTP クライアント（reqwest）で設定

**効果**:
- 無応答なサービスへの呼び出しを早期に打ち切り
- リソースの無駄遣いを防止

#### リトライ（指数バックオフ）

**実装**:
- 最大 3 回リトライ（初回を含めて合計 4 回試行）
- 指数バックオフ: 100ms, 200ms, 400ms
- 冪等な操作（GET）のみに適用

**実装の詳細**:

リトライロジックは `request_with_retry` 関数で実装されています。実装のポイントは以下の通りです：

1. **試行ループ**: `for attempt in 0..=RETRY_COUNT` で、初回（attempt=0）を含めて最大 4 回試行します。

2. **指数バックオフの計算**:
   ```rust
   let backoff = Duration::from_millis(100 * 2u64.pow(attempt - 1));
   ```
   - attempt=1（1回目のリトライ）: 100ms
   - attempt=2（2回目のリトライ）: 200ms
   - attempt=3（3回目のリトライ）: 400ms
   
   初回（attempt=0）は即座に実行し、リトライ時のみ待機します。

3. **非同期待機**: `tokio::time::sleep(backoff).await` で非ブロッキングに待機します。これにより、他のリクエストの処理をブロックしません。

4. **成功時の即時返却**: HTTP ステータスコードが成功（2xx）の場合、即座に結果を返します。リトライを続ける必要はありません。

5. **エラーの保持**: 各試行でエラーが発生した場合、最後のエラーを保持し、すべてのリトライが失敗したらそのエラーを返します。

**実装上の注意点**:
- リトライは**冪等な操作（GET）のみ**に適用しています。POST などの非冪等な操作にリトライを適用すると、重複実行のリスクがあります。
- バックオフ時間は固定値（100ms ベース）ですが、本番環境では設定可能にすることを推奨します。
- ネットワークエラーだけでなく、HTTP エラー（4xx, 5xx）もリトライ対象にしていますが、4xx（クライアントエラー）はリトライしても成功しない可能性が高いため、将来的には 5xx（サーバーエラー）のみに限定することを検討できます。

**効果**:
- 一時的なネットワーク障害やサービス再起動時の回復を支援
- バックオフにより、障害中のサービスへの負荷を軽減
- 非同期実装により、リトライ中も他のリクエストを処理可能

#### サーキットブレーカー

**サーキットブレーカーとは**:

サーキットブレーカーは、電気回路のブレーカーと同様の動作をするパターンです。下流のサービスが連続して失敗している場合、**即座にリクエストを拒否**することで、無駄なリクエスト送信とリソース消費を防ぎます。

**3つの状態**:

1. **クローズ（Closed）**: 正常動作中。リクエストを通過させる
2. **オープン（Open）**: 障害検出後。リクエストを即座に拒否
3. **ハーフオープン（Half-Open）**: 復旧試行中。1 リクエストだけ通過させて、成功すればクローズに戻る

**実装の詳細**:

このプロジェクトでは、各サービス（admin-service、teacher-service）ごとに独立したサーキットブレーカーを実装しています。

**状態管理**:

```rust
struct CircuitState {
    failures: AtomicU32,              // 連続失敗回数
    last_failure: Mutex<Option<Instant>>, // 最後の失敗時刻
}
```

- `failures`: アトミックカウンターで、スレッドセーフに失敗回数を管理
- `last_failure`: 最後の失敗時刻を記録（ハーフオープン判定に使用）

**オープン判定** (`is_open` メソッド):

1. **失敗回数チェック**: 連続 5 回失敗（`CIRCUIT_FAILURE_THRESHOLD`）に達していない場合は、クローズ状態（`false` を返す）
2. **時間経過チェック**: 5 回失敗に達している場合：
   - 最後の失敗から 30 秒（`CIRCUIT_OPEN_DURATION`）経過していない → **オープン状態**（`true` を返す）
   - 30 秒経過している → **ハーフオープン状態**（`false` を返す。1 リクエストを試行可能）

**失敗の記録** (`record_failure` メソッド):

- 失敗回数をインクリメント
- 最後の失敗時刻を記録
- 5 回に達したら警告ログを出力

**成功の記録** (`record_success` メソッド):

- 失敗回数を 0 にリセット
- 最後の失敗時刻をクリア
- **即座にクローズ状態に戻る**（段階的な回復ではなく、1 回の成功で完全復帰）

**リクエストフロー**:

```rust
pub async fn get_admin(&self, path: &str, bearer_token: Option<&str>) -> Result<...> {
    // 1. サーキットがオープンかチェック
    if self.admin_circuit.is_open() {
        return Err("circuit open (admin-service)");
    }
    
    // 2. リトライ付きでリクエスト実行
    let res = self.request_with_retry(&url, bearer_token).await;
    
    // 3. 結果に応じてサーキット状態を更新
    if res.is_ok() {
        self.admin_circuit.record_success();  // 成功 → 即座にクローズ
    } else {
        self.admin_circuit.record_failure();  // 失敗 → カウント増加
    }
    
    res
}
```

**実装のポイント**:

1. **サービスごとの独立管理**: admin-service 用と teacher-service 用で別々の `CircuitState` を保持。一方のサービスが障害でも、もう一方は正常に動作可能。

2. **即座の復帰**: 1 回の成功で即座にクローズ状態に戻る。段階的な回復ではなく、**「復旧した」と判断したら即座に通常動作に戻る**設計。

3. **ハーフオープンの実装**: 30 秒経過後は `is_open()` が `false` を返すため、次のリクエストが通過可能。成功すれば `record_success()` でクローズに戻る。

4. **アトミック操作**: `AtomicU32` を使用することで、マルチスレッド環境でも安全に失敗回数を管理。

**効果**:
- **連鎖障害の防止**: 障害中のサービスへの呼び出しを即座に拒否し、呼び出し元のサービスがタイムアウトで待機することを防ぐ
- **リソース保護**: CPU、メモリ、ネットワーク帯域などの無駄な消費を防止
- **自動回復**: サービスが復旧したら、30 秒後に自動的にハーフオープン状態になり、復旧を検証

**実装上の注意点**:
- サーキットブレーカーは**サービスごと**に独立して管理する必要がある（この実装では admin-service と teacher-service で別々）
- 閾値（5 回）と時間（30 秒）は固定値だが、本番環境では設定可能にすることを推奨
- ハーフオープン状態では、最初の 1 リクエストが成功すれば即座にクローズに戻るため、**「復旧した」と判断する基準が厳しい**。より緩やかな回復戦略が必要な場合は、複数回の成功を要求する実装も検討できる

### 5. ヘルスチェック（Liveness / Readiness）

**特徴**: Kubernetes のヘルスチェック機能を活用

**実装**:
- **Liveness Probe** (`/health`): プロセスが生きているか確認
- **Readiness Probe** (`/ready`): DB 接続が確立され、トラフィックを受け付けられるか確認

**Kubernetes マニフェストでの設定**:
```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 10

readinessProbe:
  httpGet:
    path: /ready
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 5
```

**効果**:
- **自動復旧**: プロセスが死んだら Kubernetes が自動的に再起動
- **トラフィック制御**: 準備ができていない Pod にはトラフィックを送らない
- **ローリングアップデート**: 新しい Pod が ready になってから古い Pod を停止

### 6. 設定管理（ConfigMap / Secret）

**特徴**: 設定をコードから分離し、環境ごとに変更可能にする

マイクロサービスアーキテクチャでは、**設定をコードから分離**することが重要です。同じ Docker イメージを開発・ステージング・本番環境で使い回し、環境ごとに異なる設定を注入できるようにします。

#### ConfigMap と Secret の使い分け

**ConfigMap**: 非機密情報を格納

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: admin-service-config
  namespace: edu
data:
  SERVICE_NAME: admin-service
  HTTP_PORT: "8080"
  RUST_LOG: info
  ADMIN_SERVICE_URL: http://admin-service:8080
  TEACHER_SERVICE_URL: http://teacher-service:8080
```

**Secret**: 機密情報を格納（パスワード、トークン、接続文字列など）

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: admin-service-secret
  namespace: edu
type: Opaque
stringData:
  DATABASE_URL: "postgres://edu:edu-secret-change-me@postgres.edu.svc.cluster.local:5432/edu?options=-c%20search_path%3Dadmin"
  JWT_SECRET: "your-jwt-secret-change-in-production"
```

**使い分けの基準**:
- **ConfigMap**: 公開しても問題ない情報（サービス名、ポート番号、ログレベル、URL など）
- **Secret**: 漏洩するとセキュリティリスクがある情報（パスワード、API キー、JWT シークレット、データベース接続文字列など）

#### Deployment での設定注入

**envFrom を使用した一括注入**:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: admin-service
spec:
  template:
    spec:
      containers:
        - name: admin-service
          image: admin-service:latest
          envFrom:
            - configMapRef:
                name: admin-service-config    # ConfigMap から環境変数を注入
            - secretRef:
                name: admin-service-secret    # Secret から環境変数を注入
```

**重要なポイント**: `configMapRef` と `secretRef` で指定している `name` は、**既に Kubernetes クラスタに存在している ConfigMap/Secret の名前**を参照します。YAML ファイルを直接指定するのではなく、**事前に作成されたリソースの名前**を指定します。

**適用の順序**:

ConfigMap と Secret は、Deployment を適用する**前に**作成されている必要があります：

```bash
# 1. まず ConfigMap と Secret を作成
kubectl apply -f k8s/admin-service-configmap.yaml
kubectl apply -f k8s/admin-service-secret.yaml

# 2. その後、Deployment を適用（この時点で ConfigMap/Secret が参照される）
kubectl apply -f k8s/admin-service-deployment.yaml
```

または、一度に適用することもできます（Kubernetes が依存関係を解決）：

```bash
kubectl apply -f k8s/admin-service-configmap.yaml \
              -f k8s/admin-service-secret.yaml \
              -f k8s/admin-service-deployment.yaml
```

**なぜ名前で参照するのか**:

1. **リソースの独立性**: ConfigMap/Secret と Deployment は別々のリソースとして管理できる
2. **再利用性**: 同じ ConfigMap/Secret を複数の Deployment で参照可能
3. **動的な更新**: ConfigMap/Secret を更新しても、Deployment のマニフェストを変更する必要がない
4. **名前空間の分離**: 同じ名前でも異なる namespace に存在できる

`envFrom` を使用することで、ConfigMap や Secret の**すべてのキーを環境変数として一括注入**できます。個別に `env` で指定する必要がなく、設定の追加・変更が容易です。

**注入される環境変数**:
- `SERVICE_NAME=admin-service` (ConfigMap から)
- `HTTP_PORT=8080` (ConfigMap から)
- `RUST_LOG=info` (ConfigMap から)
- `DATABASE_URL=postgres://...` (Secret から)
- `JWT_SECRET=your-jwt-secret-change-in-production` (Secret から)

#### アプリケーション側での設定読み取り

**Config 構造体** (`shared/src/config.rs`):

```rust
#[derive(Clone, Debug)]
pub struct Config {
    pub service_name: String,
    pub http_port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub rust_log: String,
}

impl Config {
    pub fn from_env() -> Result<Self, env::VarError> {
        Ok(Self {
            service_name: env::var("SERVICE_NAME")
                .unwrap_or_else(|_| "local".to_string()),
            http_port: env::var("HTTP_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
            database_url: env::var("DATABASE_URL")?,  // 必須
            jwt_secret: env::var("JWT_SECRET")?,      // 必須
            rust_log: env::var("RUST_LOG")
                .unwrap_or_else(|_| "info".to_string()),
        })
    }
}
```

**使用方法**:

```rust
// アプリケーション起動時に環境変数から設定を読み込む
let config = Config::from_env()
    .map_err(|e| format!("Failed to load config: {}", e))?;

// 設定値を使用
tracing::info!("Starting {} on port {}", config.service_name, config.http_port);
```

**設計のポイント**:
- **環境変数から読み込む**: Kubernetes が注入した環境変数を標準的な `std::env::var` で読み取る
- **デフォルト値の設定**: 一部の設定（`SERVICE_NAME`、`HTTP_PORT`、`RUST_LOG`）にはデフォルト値を設定し、ローカル開発時にも動作するようにしている
- **必須項目の明示**: `DATABASE_URL` と `JWT_SECRET` は必須として、エラーハンドリングで早期に失敗するようにしている

#### 環境ごとの設定の違い

**開発環境** (`k8s/admin-service-configmap.yaml`):
```yaml
data:
  RUST_LOG: info
  HTTP_PORT: "8080"
```

**本番環境** (例):
```yaml
data:
  RUST_LOG: warn  # 本番では詳細ログを抑制
  HTTP_PORT: "8080"
```

**Secret の環境ごとの違い**:

開発環境では `stringData` で平文を指定できますが、本番環境では：
- 外部のシークレット管理システム（HashiCorp Vault、AWS Secrets Manager など）から取得
- Kubernetes の External Secrets Operator を使用
- CI/CD パイプラインで動的に生成

#### 外部シークレット管理システムとの統合

本番環境では、AWS Secrets Manager、HashiCorp Vault、Azure Key Vault などの外部シークレット管理システムを使用することが推奨されます。Kubernetes でこれらと統合する主な方法を紹介します。

**方法1: External Secrets Operator (ESO) - 推奨**

External Secrets Operator は、複数のシークレット管理システムに対応した Kubernetes オペレーターです。AWS Secrets Manager、HashiCorp Vault、Azure Key Vault、GCP Secret Manager などに対応しています。

**セットアップ**:

```bash
# External Secrets Operator をインストール
helm repo add external-secrets https://charts.external-secrets.io
helm install external-secrets \
   external-secrets/external-secrets \
   -n external-secrets-system \
   --create-namespace
```

**AWS Secrets Manager との統合例**:

1. **SecretStore の作成** (AWS Secrets Manager への接続設定):

```yaml
apiVersion: external-secrets.io/v1beta1
kind: SecretStore
metadata:
  name: aws-secrets-manager
  namespace: edu
spec:
  provider:
    aws:
      service: SecretsManager
      region: ap-northeast-1
      auth:
        jwt:
          serviceAccountRef:
            name: external-secrets-sa
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: external-secrets-sa
  namespace: edu
  annotations:
    eks.amazonaws.com/role-arn: arn:aws:iam::ACCOUNT_ID:role/external-secrets-role
```

2. **ExternalSecret の作成** (AWS Secrets Manager から Kubernetes Secret への同期):

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: admin-service-secret
  namespace: edu
spec:
  refreshInterval: 1h  # 1時間ごとに同期
  secretStoreRef:
    name: aws-secrets-manager
    kind: SecretStore
  target:
    name: admin-service-secret  # 作成される Kubernetes Secret の名前
    creationPolicy: Owner
  data:
    - secretKey: DATABASE_URL
      remoteRef:
        key: edu/admin-service/database-url  # AWS Secrets Manager のシークレット名
    - secretKey: JWT_SECRET
      remoteRef:
        key: edu/admin-service/jwt-secret
```

3. **Deployment での使用** (既存の Secret 参照と同じ):

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: admin-service
spec:
  template:
    spec:
      containers:
        - name: admin-service
          envFrom:
            - secretRef:
                name: admin-service-secret  # ExternalSecret が作成した Secret を参照
```

**動作の流れ**:
1. External Secrets Operator が `ExternalSecret` リソースを監視
2. AWS Secrets Manager からシークレットを取得
3. Kubernetes `Secret` リソースを自動的に作成・更新
4. Deployment は通常の Secret として参照可能

**メリット**:
- **自動同期**: 設定した間隔で自動的にシークレットを更新
- **複数プロバイダー対応**: AWS、Vault、Azure などに対応
- **透過的**: アプリケーション側の変更が不要（通常の Secret として参照）

**方法2: AWS Secrets Store CSI Driver (AWS 専用)**

AWS Secrets Store CSI Driver は、AWS Secrets Manager や AWS Systems Manager Parameter Store からシークレットを取得し、Pod にボリュームとしてマウントします。

**セットアップ**:

```bash
# AWS Secrets Store CSI Driver をインストール
kubectl apply -f https://raw.githubusercontent.com/aws/secrets-store-csi-driver-provider-aws/main/deployment/secrets-store-csi-driver.yaml
```

**SecretProviderClass の作成**:

```yaml
apiVersion: secrets-store.csi.x-k8s.io/v1
kind: SecretProviderClass
metadata:
  name: admin-service-secrets
  namespace: edu
spec:
  provider: aws
  parameters:
    objects: |
      - objectName: "edu/admin-service/database-url"
        objectType: "secretsmanager"
        jmesPath: '[{"path":"DATABASE_URL","objectAlias":"DATABASE_URL"}]'
      - objectName: "edu/admin-service/jwt-secret"
        objectType: "secretsmanager"
        jmesPath: '[{"path":"JWT_SECRET","objectAlias":"JWT_SECRET"}]'
  secretObjects:
    - secretName: admin-service-secret
      type: Opaque
      data:
        - objectName: DATABASE_URL
          key: DATABASE_URL
        - objectName: JWT_SECRET
          key: JWT_SECRET
```

**Deployment での使用**:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: admin-service
spec:
  template:
    spec:
      serviceAccountName: admin-service-sa  # IAM ロールを持つ ServiceAccount
      containers:
        - name: admin-service
          volumeMounts:
            - name: secrets-store
              mountPath: /mnt/secrets-store
              readOnly: true
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: admin-service-secret
                  key: DATABASE_URL
            - name: JWT_SECRET
              valueFrom:
                secretKeyRef:
                  name: admin-service-secret
                  key: JWT_SECRET
      volumes:
        - name: secrets-store
          csi:
            driver: secrets-store.csi.k8s.io
            readOnly: true
            volumeAttributes:
              secretProviderClass: admin-service-secrets
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: admin-service-sa
  namespace: edu
  annotations:
    eks.amazonaws.com/role-arn: arn:aws:iam::ACCOUNT_ID:role/admin-service-role
```

**方法3: アプリケーション側で直接取得**

アプリケーション側で AWS SDK を使って直接 Secrets Manager から取得する方法もあります。

**Rust での実装例** (aws-sdk-secretsmanager を使用):

```rust
use aws_sdk_secretsmanager::Client as SecretsClient;

async fn load_secrets_from_aws() -> Result<Config, Error> {
    let config = aws_config::load_from_env().await;
    let client = SecretsClient::new(&config);
    
    // DATABASE_URL を取得
    let db_url_resp = client
        .get_secret_value()
        .secret_id("edu/admin-service/database-url")
        .send()
        .await?;
    let database_url = db_url_resp.secret_string().unwrap();
    
    // JWT_SECRET を取得
    let jwt_resp = client
        .get_secret_value()
        .secret_id("edu/admin-service/jwt-secret")
        .send()
        .await?;
    let jwt_secret = jwt_resp.secret_string().unwrap();
    
    Ok(Config {
        database_url: database_url.to_string(),
        jwt_secret: jwt_secret.to_string(),
        // ...
    })
}
```

**メリット・デメリット**:

| 方法 | メリット | デメリット |
|------|---------|-----------|
| External Secrets Operator | 自動同期、透過的、複数プロバイダー対応 | 追加のオペレーターが必要 |
| Secrets Store CSI Driver | AWS ネイティブ、ボリュームマウント | AWS 専用、設定が複雑 |
| アプリケーション側で取得 | 柔軟性が高い | アプリケーションコードの変更が必要、キャッシュが必要 |

**推奨アプローチ**:

- **AWS EKS を使用している場合**: External Secrets Operator または Secrets Store CSI Driver
- **複数のクラウドプロバイダーに対応したい場合**: External Secrets Operator
- **シンプルな構成で AWS のみ**: Secrets Store CSI Driver
- **カスタム要件がある場合**: アプリケーション側で直接取得

**セキュリティのベストプラクティス**:

1. **IAM ロールの最小権限**: ServiceAccount に必要なシークレットへのアクセスのみを許可
2. **シークレットのローテーション**: 定期的にシークレットをローテーションし、自動同期を有効化
3. **監査ログ**: CloudTrail などでシークレットへのアクセスを監視
4. **暗号化**: Secrets Manager の暗号化機能を有効化

#### 設定の更新とロールアウト

**ConfigMap / Secret の更新**:

```bash
# ConfigMap を更新
kubectl apply -f k8s/admin-service-configmap.yaml

# Secret を更新（base64 エンコードが必要な場合）
kubectl create secret generic admin-service-secret \
  --from-literal=DATABASE_URL='postgres://...' \
  --from-literal=JWT_SECRET='new-secret' \
  --dry-run=client -o yaml | kubectl apply -f -
```

**Pod の再起動**:

ConfigMap や Secret を更新しても、**既存の Pod には自動的に反映されません**。環境変数は Pod 起動時に注入されるため、以下のいずれかの方法で Pod を再起動する必要があります：

```bash
# 方法1: Deployment を再起動
kubectl rollout restart deployment/admin-service -n edu

# 方法2: Pod を削除（Deployment が自動的に再作成）
kubectl delete pod -l app=admin-service -n edu
```

**注意**: 本番環境では、設定変更時に**段階的なロールアウト**（ローリングアップデート）を行うことで、ダウンタイムを避けられます。

#### メリットと注意点

**メリット**:
- **環境分離**: 同じ Docker イメージを開発・ステージング・本番で使い回せる
- **セキュリティ**: シークレットをコードリポジトリに含めない（`.gitignore` で Secret ファイルを除外可能）
- **運用性**: 設定変更時にイメージを再ビルドする必要がない
- **一元管理**: Kubernetes で設定を一元管理でき、複数の Pod に同じ設定を適用可能

**注意点**:
- **Secret の暗号化**: Secret は base64 エンコードされているだけで、**暗号化されていない**。Kubernetes の etcd で暗号化することを推奨（`EncryptionConfiguration` を使用）
- **Secret のアクセス制御**: RBAC で Secret へのアクセスを制限する
- **設定の検証**: アプリケーション起動時に設定の妥当性をチェックし、不正な設定で起動しないようにする
- **外部シークレット管理**: 本番環境では、HashiCorp Vault や AWS Secrets Manager などの外部シークレット管理システムとの統合を検討
- **設定のバージョン管理**: ConfigMap や Secret の変更履歴を Git で管理し、設定変更の追跡可能性を確保

#### 実際の運用でのベストプラクティス

1. **設定の階層化**: 
   - デフォルト値（コード内）
   - 環境共通設定（ConfigMap）
   - 環境固有設定（環境ごとの ConfigMap）
   - 機密情報（Secret）

2. **設定の検証**:
   - アプリケーション起動時に必須設定の存在を確認
   - 設定値の妥当性チェック（ポート番号の範囲、URL の形式など）

3. **設定変更の影響範囲**:
   - 設定変更が既存の Pod に影響しないことを理解する
   - 重要な設定変更時は、段階的なロールアウトを計画する

4. **Secret のローテーション**:
   - JWT シークレットやデータベースパスワードの定期的なローテーション計画
   - ローテーション時のダウンタイムを最小化する戦略

### 7. Ingress によるルーティング

**特徴**: 単一エントリポイントから各サービスにルーティング

**実装**:
- Ingress でパスベースルーティング:
  - `/api/admin/*` → admin-service
  - `/api/teacher/*` → teacher-service
  - `/api/student/*` → student-service

**メリット**:
- **統一された API エンドポイント**: クライアントは 1 つのホスト名で全 API にアクセス
- **TLS 終端**: Ingress で TLS を処理（本番環境）
- **ロードバランシング**: Ingress コントローラーが負荷分散

### 8. サービス間通信の設計

**特徴**: 同期 REST API でサービス間通信

**実装例**:
- teacher-service が課題を作成する際、admin-service にコース存在確認
- student-service が提出する際、teacher-service に課題存在確認

**設計判断**:
- **同期通信を選択**: シンプルで、エラーハンドリングが明確
- **JWT 転送**: 元のリクエストの認証情報を保持
- **エラーハンドリング**: 下流サービスが失敗したら、適切な HTTP ステータスコード（502 Bad Gateway）を返す

**将来の拡張**:
- 非同期通信（メッセージキュー）への移行も可能
- イベント駆動アーキテクチャへの発展も検討できる

## 実装で学んだこと

### 1. マイクロサービスの「独立性」は相対的

完全に独立したマイクロサービスは現実的ではありません。このプロジェクトでは：
- **データ**: スキーマ分離で独立性を確保（将来的に DB 分離も可能）
- **認証**: JWT シークレットを共有（認証サービスに移行可能）
- **通信**: 同期 REST（非同期に移行可能）

重要なのは、**将来の変更に備えた設計**です。

### 2. レジリエンスパターンは必須

マイクロサービスでは、サービス間通信の失敗は避けられません。タイムアウト、リトライ、サーキットブレーカーは**標準装備**として実装すべきです。

特にサーキットブレーカーは、1 つのサービスの障害が全体に波及するのを防ぐ重要なパターンです。

### 3. Kubernetes の標準機能を最大限活用

外部ツールに頼らず、Kubernetes の標準機能（DNS、ConfigMap、Secret、Probe）を活用することで、**シンプルで保守しやすい**システムを構築できます。

### 4. 開発体験も重要

このプロジェクトでは、以下の開発支援スクリプトを用意しました：
- **ローカルセットアップスクリプト**: ワンコマンドで環境構築
- **再デプロイスクリプト**: コード変更を即座に反映（port-forward も自動再起動）
- **インテグレーションテストスクリプト**: 3 サービスの連携を自動検証

マイクロサービスは複雑になりがちですが、**開発体験を改善するツール**を用意することで、開発効率を大幅に向上できます。

## まとめ

このプロジェクトを通じて、以下のマイクロサービスのパターンを実践的に学びました：

1. **Kubernetes ネイティブなサービスディスカバリ**: シンプルで標準的
2. **サービスごとのデータ所有**: スキーマ分離から始める
3. **JWT 認証と RBAC**: 分散認証の実装
4. **レジリエンスパターン**: タイムアウト、リトライ、サーキットブレーカー
5. **ヘルスチェック**: Liveness / Readiness の活用
6. **設定管理**: ConfigMap / Secret による環境分離
7. **Ingress ルーティング**: 単一エントリポイント
8. **サービス間通信**: 同期 REST とエラーハンドリング

これらのパターンは、マイクロサービスアーキテクチャを構築する上で**実践的で標準的なアプローチ**です。理論だけでなく、実際に手を動かして実装することで、各パターンの重要性と実装の難しさを理解できます。

Rust で実装したことで、**型安全性**と**パフォーマンス**の両立も実感できましたが、これらのパターン自体は言語に依存しない、**マイクロサービスアーキテクチャの本質**です。

## 参考リソース

- [プロジェクトリポジトリ](https://github.com/kojikokojiko/microservice-practice)
