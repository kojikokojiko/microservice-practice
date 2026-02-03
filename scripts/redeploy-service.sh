#!/usr/bin/env bash
# コード変更を反映: 指定サービスをビルドして K8s デプロイを再起動する（minikube 想定）

set -e
cd "$(dirname "$0")/.."

SERVICE="${1:-}"
if [[ -z "$SERVICE" ]]; then
  echo "Usage: $0 <admin-service|teacher-service|student-service|all>"
  exit 1
fi

echo "=== minikube の Docker でビルド ==="
eval $(minikube docker-env)

build_one() {
  local name=$1
  local dockerfile=$2
  local port=$3
  echo "ビルド中: $name ..."
  docker build -f "$dockerfile" -t "${name}:latest" . --progress=plain
  echo "再起動: deployment/$name -n edu"
  kubectl rollout restart "deployment/$name" -n edu
  echo "Pod が Ready になるまで待機中..."
  kubectl rollout status "deployment/$name" -n edu --timeout=120s
  
  # 古い port-forward を kill して再起動
  echo "port-forward を再起動: localhost:$port -> $name:8080"
  pkill -f "kubectl port-forward.*svc/$name" || true
  kubectl port-forward -n edu "svc/$name" "$port:8080" > /dev/null 2>&1 &
  sleep 1
  echo "✓ $name: localhost:$port で利用可能"
}

case "$SERVICE" in
  admin-service)
    build_one admin-service Dockerfile.admin-service 8080
    ;;
  teacher-service)
    build_one teacher-service Dockerfile.teacher-service 8081
    ;;
  student-service)
    build_one student-service Dockerfile.student-service 8082
    ;;
  all)
    build_one admin-service Dockerfile.admin-service 8080
    build_one teacher-service Dockerfile.teacher-service 8081
    build_one student-service Dockerfile.student-service 8082
    ;;
  *)
    echo "Unknown service: $SERVICE"
    echo "Usage: $0 <admin-service|teacher-service|student-service|all>"
    exit 1
    ;;
esac

echo ""
echo "=== 反映完了。すぐに API を試せます ==="
