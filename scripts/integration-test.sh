#!/usr/bin/env bash
# インテグレーションテスト: 3 サービスの連携を検証（垂直スライス）
# 前提: Pod が Running で port-forward が動いている状態

set -e
cd "$(dirname "$0")/.."

# 色付き出力
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SECRET="${JWT_SECRET:-your-jwt-secret-change-in-production}"
ADMIN_URL="${ADMIN_URL:-http://localhost:8080}"
TEACHER_URL="${TEACHER_URL:-http://localhost:8081}"
STUDENT_URL="${STUDENT_URL:-http://localhost:8082}"

echo "=========================================="
echo "  インテグレーションテスト (垂直スライス)"
echo "=========================================="
echo ""

# ヘルスチェック
echo "Step 0: ヘルスチェック"
echo "  admin-service..."
ADMIN_HEALTH=$(curl -s -w "%{http_code}" -o /dev/null "$ADMIN_URL/health" || echo "000")
if [[ "$ADMIN_HEALTH" != "200" ]]; then
  echo -e "${RED}✗ admin-service ヘルスチェック失敗 (HTTP $ADMIN_HEALTH)${NC}"
  echo "  port-forward が動いているか確認してください: kubectl port-forward -n edu svc/admin-service 8080:8080"
  exit 1
fi
echo -e "${GREEN}✓ admin-service OK${NC}"

echo "  teacher-service..."
TEACHER_HEALTH=$(curl -s -w "%{http_code}" -o /dev/null "$TEACHER_URL/health" || echo "000")
if [[ "$TEACHER_HEALTH" != "200" ]]; then
  echo -e "${RED}✗ teacher-service ヘルスチェック失敗 (HTTP $TEACHER_HEALTH)${NC}"
  exit 1
fi
echo -e "${GREEN}✓ teacher-service OK${NC}"

echo "  student-service..."
STUDENT_HEALTH=$(curl -s -w "%{http_code}" -o /dev/null "$STUDENT_URL/health" || echo "000")
if [[ "$STUDENT_HEALTH" != "200" ]]; then
  echo -e "${RED}✗ student-service ヘルスチェック失敗 (HTTP $STUDENT_HEALTH)${NC}"
  exit 1
fi
echo -e "${GREEN}✓ student-service OK${NC}"
echo ""

# Step 1: 管理者がコース作成
echo "Step 1: 管理者がコース作成 (admin-service)"
TOKEN_ADMIN=$(./scripts/gen-jwt.sh admin "$SECRET")
COURSE_NAME="Integration Test Course $(date +%s)"
COURSE_RESP=$(curl -s -X POST "$ADMIN_URL/api/admin/courses" \
  -H "Authorization: Bearer $TOKEN_ADMIN" \
  -H "Content-Type: application/json" \
  -d "{\"name\":\"$COURSE_NAME\"}")

COURSE_ID=$(echo "$COURSE_RESP" | grep -o '"id":"[^"]*"' | head -1 | sed 's/"id":"\([^"]*\)"/\1/')
if [[ -z "$COURSE_ID" ]]; then
  echo -e "${RED}✗ コース作成失敗${NC}"
  echo "  レスポンス: $COURSE_RESP"
  exit 1
fi
echo -e "${GREEN}✓ コース作成成功: $COURSE_ID${NC}"
echo "  コース名: $COURSE_NAME"
echo ""

# Step 2: 教師が課題作成（admin-service にコース存在確認）
echo "Step 2: 教師が課題作成 (teacher-service → admin-service)"
TOKEN_TEACHER=$(./scripts/gen-jwt.sh teacher "$SECRET")
ASSIGNMENT_TITLE="Integration Test Assignment $(date +%s)"
ASSIGN_RESP=$(curl -s -X POST "$TEACHER_URL/api/teacher/courses/$COURSE_ID/assignments" \
  -H "Authorization: Bearer $TOKEN_TEACHER" \
  -H "Content-Type: application/json" \
  -d "{\"title\":\"$ASSIGNMENT_TITLE\"}")

ASSIGNMENT_ID=$(echo "$ASSIGN_RESP" | grep -o '"id":"[^"]*"' | head -1 | sed 's/"id":"\([^"]*\)"/\1/')
if [[ -z "$ASSIGNMENT_ID" ]]; then
  echo -e "${RED}✗ 課題作成失敗${NC}"
  echo "  レスポンス: $ASSIGN_RESP"
  exit 1
fi
echo -e "${GREEN}✓ 課題作成成功: $ASSIGNMENT_ID${NC}"
echo "  課題名: $ASSIGNMENT_TITLE"
echo ""

# Step 3: 学生が提出（teacher-service に課題存在確認）
echo "Step 3: 学生が提出 (student-service → teacher-service)"
TOKEN_STUDENT=$(./scripts/gen-jwt.sh student "$SECRET")
SUBMISSION_CONTENT="My test submission at $(date)"
SUBMIT_RESP=$(curl -s -X POST "$STUDENT_URL/api/student/assignments/$ASSIGNMENT_ID/submissions" \
  -H "Authorization: Bearer $TOKEN_STUDENT" \
  -H "Content-Type: application/json" \
  -d "{\"content\":\"$SUBMISSION_CONTENT\"}")

SUBMISSION_ID=$(echo "$SUBMIT_RESP" | grep -o '"id":"[^"]*"' | head -1 | sed 's/"id":"\([^"]*\)"/\1/')
if [[ -z "$SUBMISSION_ID" ]]; then
  echo -e "${RED}✗ 提出失敗${NC}"
  echo "  レスポンス: $SUBMIT_RESP"
  exit 1
fi
echo -e "${GREEN}✓ 提出成功: $SUBMISSION_ID${NC}"
echo "  提出内容: $SUBMISSION_CONTENT"
echo ""

# Step 4: 各エンドポイントで取得確認
echo "Step 4: 作成したリソースを取得して検証"

echo "  コース取得 (admin-service)..."
COURSE_GET=$(curl -s "$ADMIN_URL/api/admin/courses/$COURSE_ID" \
  -H "Authorization: Bearer $TOKEN_ADMIN")
if echo "$COURSE_GET" | grep -q "\"id\":\"$COURSE_ID\""; then
  echo -e "${GREEN}✓ コース取得成功${NC}"
else
  echo -e "${RED}✗ コース取得失敗${NC}"
  echo "  レスポンス: $COURSE_GET"
  exit 1
fi

echo "  課題取得 (teacher-service)..."
ASSIGN_GET=$(curl -s "$TEACHER_URL/api/teacher/assignments/$ASSIGNMENT_ID" \
  -H "Authorization: Bearer $TOKEN_TEACHER")
if echo "$ASSIGN_GET" | grep -q "\"id\":\"$ASSIGNMENT_ID\""; then
  echo -e "${GREEN}✓ 課題取得成功${NC}"
else
  echo -e "${RED}✗ 課題取得失敗${NC}"
  echo "  レスポンス: $ASSIGN_GET"
  exit 1
fi

echo ""
echo "=========================================="
echo -e "${GREEN}✓ すべてのテストが成功しました！${NC}"
echo "=========================================="
echo ""
echo "検証内容:"
echo "  1. admin-service でコース作成"
echo "  2. teacher-service が admin-service を呼んで課題作成（サービス間連携 1）"
echo "  3. student-service が teacher-service を呼んで提出（サービス間連携 2）"
echo "  4. 各リソースの取得確認"
echo ""
echo "作成されたリソース:"
echo "  - コース ID: $COURSE_ID"
echo "  - 課題 ID: $ASSIGNMENT_ID"
echo "  - 提出 ID: $SUBMISSION_ID"
