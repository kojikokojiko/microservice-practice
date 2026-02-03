# SPEC.md --- Rust Microservices on Kubernetes (Education Platform)

## 0. Goal

Build a minimal-but-realistic microservices system in Rust running on
Kubernetes. Domain: education platform with three services: -
admin-service - teacher-service - student-service

Primary objective: learn Kubernetes-native service discovery (DNS),
per-service data ownership, and async Rust web APIs.

## 1. Scope (MVP)

### Must-have

-   3 independent HTTP services (Rust, async) deployed to Kubernetes
-   Kubernetes-native service discovery (no Eureka)
-   Each service owns its own schema (single DB, schemas: admin, teacher, student)
-   Vertical slice:
    1)  Admin creates Course
    2)  Teacher creates Assignment
    3)  Student submits Submission
-   JWT verification with roles (admin, teacher, student)
-   Health/readiness endpoints
-   K8s manifests runnable on kind/minikube

## 2. Tech Decisions

-   axum, tokio, reqwest
-   sqlx + PostgreSQL
-   config + env vars
-   tracing

## 3. Architecture

-   Synchronous REST between services
-   DNS names:
    -   http://admin-service:8080
    -   http://teacher-service:8080
    -   http://student-service:8080

## 4. APIs (MVP)

Admin: - POST /api/admin/courses - GET /api/admin/courses/{course_id}

Teacher: - POST /api/teacher/courses/{course_id}/assignments - GET
/api/teacher/assignments/{assignment_id}

Student: - POST /api/student/assignments/{assignment_id}/submissions

## 5. Data Model

admin.courses teacher.assignments student.submissions

## 6. Kubernetes

-   Namespace: edu
-   One Postgres, one database, three schemas (admin, teacher, student)
-   Deployment + Service + ConfigMap + Secret per service
-   Ingress (or Gateway) as single entrypoint; route /api/admin/*, /api/teacher/*, /api/student/* to respective services

## 7. Cross-cutting & Resilience (must implement)

-   **Service Discovery (K8s DNS)**  
    Services call each other via DNS: `http://<service-name>:8080` (same namespace). No Eureka or external discovery.

-   **Config (ConfigMap / Secret)**  
    ConfigMap: non-sensitive (SERVICE_NAME, HTTP_PORT, RUST_LOG, DB host). Secret: DATABASE_URL, JWT_SECRET. Mount as env vars.

-   **Auth (JWT / OIDC)**  
    JWT verification on every protected API: verify signature (JWT_SECRET), validate exp/iss, extract role (admin/teacher/student). MVP: shared secret; optional later: OIDC issuer (e.g. Keycloak).

-   **Gateway / Ingress**  
    One Ingress (or API Gateway) in front; path-based routing to admin-service, teacher-service, student-service. TLS optional for MVP.

-   **Timeout**  
    Outbound HTTP (reqwest): set connect_timeout and timeout (e.g. 5s / 30s). Inbound (axum): use timeout middleware so slow requests are cut.

-   **Retry**  
    Outbound calls: retry with exponential backoff (e.g. 3 retries, only for idempotent or safe-to-retry operations). Use reqwest retry or tower.

-   **Circuit Breaker**  
    Outbound calls to other services: use circuit breaker (e.g. tower) so repeated failures open the circuit and avoid cascading failure; half-open to probe recovery.

-   **Health Check**  
    Liveness: GET /health (or /live) — process up. Readiness: GET /ready — DB connected and ready to serve. K8s: livenessProbe and readinessProbe on these endpoints.

## 8. Env Vars

-   SERVICE_NAME
-   HTTP_PORT
-   DATABASE_URL
-   JWT_SECRET
-   RUST_LOG

## 9. Acceptance Criteria

-   All services healthy (liveness/readiness pass)
-   Vertical slice works
-   Services communicate via K8s DNS
-   Config from ConfigMap/Secret; auth via JWT; timeouts, retry, and circuit breaker on outbound calls; Ingress routes traffic