# アプリケーションの起動手順

**標準のローカルセットアップ = minikube + 同じ K8s マニフェスト** です。Discovery・ConfigMap・Secret・Health など、本番に近い動きをそのまま再現できます。

---

## 標準: minikube でローカルセットアップ（推奨）

### 前提

- [minikube](https://minikube.sigs.k8s.io/docs/start/) と kubectl が入っていること
- Docker が動いていること（minikube のドライバで利用）

### 一括実行（スクリプト）

```bash
cd /Users/koji.iwase/microservice-practice
./scripts/run-local-setup.sh
```

スクリプトが終わったら、表示される「アクセス方法」に従って port-forward または Ingress で API を叩きます。

### コード変更を反映するとき（再ビルド・再起動）

サービスを変更したあと、そのサービスだけビルドしてデプロイを再起動するスクリプトがあります（minikube 想定）:

```bash
./scripts/redeploy-service.sh admin-service   # 1 サービスだけ
./scripts/redeploy-service.sh all             # 3 サービスまとめて
```

### 手動でやる場合

1. **minikube 起動**

   ```bash
   minikube start
   ```

2. **minikube の Docker でイメージをビルド**（クラスタ内で同じイメージ名が使えるようにする）

   ```bash
   cd /Users/koji.iwase/microservice-practice
   eval $(minikube docker-env)
   docker build -f Dockerfile.admin-service -t admin-service:latest .
   docker build -f Dockerfile.teacher-service -t teacher-service:latest .
   docker build -f Dockerfile.student-service -t student-service:latest .
   ```

3. **K8s マニフェストを適用（順番どおり）**

   ```bash
   kubectl apply -f k8s/namespace.yaml
   kubectl apply -f k8s/postgres-configmap.yaml -f k8s/postgres-secret.yaml -f k8s/postgres-deployment.yaml
   kubectl apply -f k8s/migrations-configmap.yaml -f k8s/migrations-job.yaml
   kubectl wait --for=condition=complete job/run-migrations -n edu --timeout=120s
   kubectl apply -f k8s/admin-service-configmap.yaml -f k8s/admin-service-secret.yaml -f k8s/admin-service-deployment.yaml
   kubectl apply -f k8s/teacher-service-configmap.yaml -f k8s/teacher-service-secret.yaml -f k8s/teacher-service-deployment.yaml
   kubectl apply -f k8s/student-service-configmap.yaml -f k8s/student-service-secret.yaml -f k8s/student-service-deployment.yaml
   kubectl apply -f k8s/ingress.yaml
   ```

4. **Ingress を使う場合（任意）**

   ```bash
   minikube addons enable ingress
   # Ingress が有効になったら、必要に応じて minikube tunnel や /etc/hosts でホスト名を設定
   ```

5. **アクセス**

   - **port-forward（手軽）**: 別ターミナルで以下を実行し、localhost で API を叩く。

     ```bash
     kubectl port-forward -n edu svc/admin-service 8080:8080 &
     kubectl port-forward -n edu svc/teacher-service 8081:8080 &
     kubectl port-forward -n edu svc/student-service 8082:8080 &
     ```

   - 垂直スライスの curl 例は「動作確認」の節を参照。

---

## 動作確認（垂直スライス）

admin / teacher / student に port-forward している前提（上記の通り 8080 / 8081 / 8082）。

**重要**: port-forward のあと、**全 Pod が Ready になるまで 30 秒〜1 分ほど待ってから** curl を実行してください。  
`course service unavailable` が出る場合は、teacher-service が admin-service に届いていません。下記「トラブルシュート」を参照。

```bash
cd /Users/koji.iwase/microservice-practice
SECRET="your-jwt-secret-change-in-production"

# 1) 管理者がコース作成
TOKEN_ADMIN=$(./scripts/gen-jwt.sh admin "$SECRET")
COURSE_RESP=$(curl -s -X POST http://localhost:8080/api/admin/courses \
  -H "Authorization: Bearer $TOKEN_ADMIN" -H "Content-Type: application/json" -d '{"name":"Math 101"}')
COURSE_ID=$(echo "$COURSE_RESP" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
echo "COURSE_ID=$COURSE_ID"

# 2) 教師が課題作成
TOKEN_TEACHER=$(./scripts/gen-jwt.sh teacher "$SECRET")
ASSIGN_RESP=$(curl -s -X POST "http://localhost:8081/api/teacher/courses/$COURSE_ID/assignments" \
  -H "Authorization: Bearer $TOKEN_TEACHER" -H "Content-Type: application/json" -d '{"title":"Homework 1"}')
ASSIGNMENT_ID=$(echo "$ASSIGN_RESP" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
echo "ASSIGNMENT_ID=$ASSIGNMENT_ID"

# 3) 学生が提出
TOKEN_STUDENT=$(./scripts/gen-jwt.sh student "$SECRET")
curl -s -X POST "http://localhost:8082/api/student/assignments/$ASSIGNMENT_ID/submissions" \
  -H "Authorization: Bearer $TOKEN_STUDENT" -H "Content-Type: application/json" -d '{"content":"My answer"}'
```

**COURSE_ID や ASSIGNMENT_ID が空になる場合**  
port-forward が動いていないと curl が失敗します（exit 56 など）。別ターミナルで以下を実行してから再度試してください。

```bash
kubectl port-forward -n edu svc/admin-service 8080:8080 &
kubectl port-forward -n edu svc/teacher-service 8081:8080 &
kubectl port-forward -n edu svc/student-service 8082:8080 &
```

エラー確認用: `echo "$COURSE_RESP"` でレスポンスの中身を確認できます（接続失敗時は空、API エラー時はメッセージが返ります）。

### 自動インテグレーションテスト

3 サービスの連携を一括で検証するスクリプトがあります。

```bash
./scripts/integration-test.sh
```

このスクリプトは次を自動で行います:

1. 各サービスのヘルスチェック
2. 管理者がコース作成（admin-service）
3. 教師が課題作成（teacher-service → admin-service 連携）
4. 学生が提出（student-service → teacher-service 連携）
5. 作成したリソースの取得確認

すべて成功すれば緑色で「✓ すべてのテストが成功しました！」が表示されます。失敗した場合はどのステップで失敗したかとレスポンスが表示されます。

### トークンは実際どうやって発行する？（ユーザーが API を使う場合）

いまの `gen-jwt.sh` は **検証用** で、CLI からテスト用トークンを発行しているだけです。

**実際にユーザー（フロントや他システム）が API を使うとき**は、次のような流れになります。

1. **ログイン API（認証サービス）** を用意する  
   - 例: `POST /auth/login` で `{ "email", "password" }` を受け取り、照合後に JWT を発行して返す。  
   - または OAuth2 / OIDC（Google ログインなど）の認可サーバーに任せる。

2. **クライアント**は、ログイン API から受け取った JWT を `Authorization: Bearer <token>` で各 API（admin / teacher / student）に付けて呼ぶ。

このリポジトリには **ログイン・トークン発行のエンドポイントは含まれていません**。各サービスは「渡された JWT の検証」だけをしています。  
本番では、**Auth0・Keycloak・Cognito などの IdP** や、自前の **auth-service**（ログイン + JWT 発行）を前面に置き、そこで発行したトークンをこの API 群に渡す形になります。

### サービスは「どのトークンが正しいか」をどうやって知るの？

**秘密鍵（JWT_SECRET）を共有して、署名で検証しています。** トークン一覧を DB で持っているわけではありません。

1. **JWT の形**  
   JWT は `ヘッダ.ペイロード.署名` の 3 部分です。ペイロードに `sub`（ユーザーID）・`role`・`exp`（有効期限）などが入ります。

2. **発行側（gen-jwt.sh やログイン API）**  
   - ペイロードを決めて、**JWT_SECRET** で HMAC-SHA256 の署名を作り、`ヘッダ.ペイロード.署名` をクライアントに渡します。  
   - `gen-jwt.sh` の第 2 引数がその秘密鍵（K8s の Secret の `JWT_SECRET` と合わせる必要があります）。

3. **サービス側（admin / teacher / student）**  
   - 環境変数 **JWT_SECRET** を読み、`Authorization: Bearer <token>` のトークンに対して **同じ秘密鍵で署名を検証**します（`shared/src/auth.rs` の `verify_jwt`）。  
   - 署名が一致すれば「この秘密鍵を知っている主体が発行したトークン」と判断し、`exp` が過ぎていなければ正しいトークンとして `Claims`（`sub`, `role` など）を取り出します。  
   - 署名が違う or 期限切れなら `401 Unauthorized` を返します。

つまり、**「正しいトークンか」は「同じ JWT_SECRET で署名されているか＋有効期限内か」で判定**しています。発行側と検証側で JWT_SECRET を揃えておく必要があります（QUICKSTART の curl で使う `SECRET` と、K8s の各サービスの `JWT_SECRET` が同じである必要がある理由です）。

---

## トラブルシュート: "course service unavailable"

教師が課題を作成するとき、teacher-service は **admin-service** に「コースが存在するか」を問い合わせます。  
`course service unavailable` は「その HTTP 呼び出しが失敗した」という意味です。

**1. 全 Pod が Ready か確認**

```bash
kubectl get pods -n edu
```

`admin-service`, `teacher-service`, `postgres` がすべて `Running` かつ `READY` が `1/1` になるまで待ってから、もう一度 curl を実行してください。

**2. クラスタ内から admin-service に届くか確認**

```bash
kubectl run -it --rm debug --image=curlimages/curl --restart=Never -n edu -- curl -s http://admin-service:8080/health
```

`ok` と出れば admin-service には届いています。届かない場合は admin-service のログを確認してください。

```bash
kubectl logs -n edu -l app=admin-service --tail=50
```

**3. JWT のシークレットが K8s の Secret と一致しているか**

curl で使っている `SECRET` と、k8s の `admin-service-secret` / `teacher-service-secret` の `JWT_SECRET` が同じである必要があります。  
デフォルトは `your-jwt-secret-change-in-production` です。

---

## 別案: kind で同じ K8s セットアップ

minikube の代わりに kind を使う場合も、**同じ k8s マニフェスト**で同じ動きになります。

1. `kind create cluster`
2. イメージをビルドして kind にロード:  
   `docker build ...` → `kind load docker-image admin-service:latest ...`
3. 上と同じ順で `kubectl apply -f k8s/...`
4. Ingress を使う場合は [ingress-nginx の kind 用マニフェスト](https://kubernetes.github.io/ingress-nginx/deploy/) を apply

---

## 簡易ローカル（Discovery なし・参考）

K8s を使わず、Postgres + 3 つの `cargo run` で「とりあえず API を試す」方法です。**Service Discovery はありません**（URL を環境変数で静的に指定）。

- Postgres: `docker run -d --name postgres-edu -e POSTGRES_USER=edu -e POSTGRES_PASSWORD=edu -e POSTGRES_DB=edu -p 5432:5432 postgres:16-alpine`
- マイグレーション: `export PGPASSWORD=edu` のあと `psql -h localhost -U edu -d edu -f migrations/001_create_schemas.sql` など 4 ファイルを実行
- 3 サービス: 別ターミナルでそれぞれ `DATABASE_URL`（search_path=admin/teacher/student）、`JWT_SECRET`、teacher/student は `HTTP_PORT` と `ADMIN_SERVICE_URL` / `TEACHER_SERVICE_URL` を設定して `cargo run -p admin-service` / `teacher-service` / `student-service`

詳細は [README.md](../README.md) の「Local run」を参照。
