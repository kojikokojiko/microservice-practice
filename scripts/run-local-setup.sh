#!/usr/bin/env bash
# 標準ローカルセットアップ: minikube + 同じ K8s マニフェスト（Discovery / ConfigMap / Secret 等を再現）

set -e
cd "$(dirname "$0")/.."

echo "=== minikube 起動 ==="
if ! minikube status &>/dev/null; then
  minikube start
else
  echo "minikube は既に起動しています。"
fi

echo ""
echo "=== イメージを minikube の Docker でビルド ==="
echo "（初回は5-10分かかります。2回目以降はキャッシュで速くなります）"
eval $(minikube docker-env)
echo "admin-service をビルド中..."
docker build -f Dockerfile.admin-service -t admin-service:latest . --progress=plain
echo "teacher-service をビルド中..."
docker build -f Dockerfile.teacher-service -t teacher-service:latest . --progress=plain
echo "student-service をビルド中..."
docker build -f Dockerfile.student-service -t student-service:latest . --progress=plain

echo ""
echo "=== K8s マニフェストを適用 ==="
kubectl apply -f k8s/namespace.yaml
kubectl apply -f k8s/postgres-configmap.yaml -f k8s/postgres-secret.yaml -f k8s/postgres-deployment.yaml
kubectl apply -f k8s/migrations-configmap.yaml -f k8s/migrations-job.yaml
echo "マイグレーション完了を待機中..."
kubectl wait --for=condition=complete job/run-migrations -n edu --timeout=120s
kubectl apply -f k8s/admin-service-configmap.yaml -f k8s/admin-service-secret.yaml -f k8s/admin-service-deployment.yaml
kubectl apply -f k8s/teacher-service-configmap.yaml -f k8s/teacher-service-secret.yaml -f k8s/teacher-service-deployment.yaml
kubectl apply -f k8s/student-service-configmap.yaml -f k8s/student-service-secret.yaml -f k8s/student-service-deployment.yaml
kubectl apply -f k8s/ingress.yaml

echo ""
echo "=== Ingress アドオン（任意） ==="
minikube addons enable ingress 2>/dev/null || true

echo ""
echo "=== デプロイ完了。Pod が Ready になるまで少し待つ場合があります。 ==="
echo ""
echo "--- アクセス方法（port-forward） ---"
echo "次のコマンドを実行すると、localhost から API を叩けます:"
echo ""
echo "  kubectl port-forward -n edu svc/admin-service 8080:8080 &"
echo "  kubectl port-forward -n edu svc/teacher-service 8081:8080 &"
echo "  kubectl port-forward -n edu svc/student-service 8082:8080 &"
echo ""
echo "その後、docs/QUICKSTART-ja.md の「動作確認（垂直スライス）」の curl を実行してください。"
