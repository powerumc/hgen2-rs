# httpgen

`httpgen`은 NIC에 raw Ethernet/TCP/HTTP 패킷을 직접 생성해 전송하는 HTTP traffic generator입니다.

사용 사례
- 네트워크 패킷 탐지 시뮬레이션
- TAP 기반 네트워크 미러링 환경 시뮬레이션
- 실제 NIC로 전송되는 L2 이더넷 프레임 기반 트래픽 시뮬레이션
- 의미있는 HTTP 요청/응답 트래픽 시뮬레이션

## Usage

릴리즈 페이지에서 다운로드할 수 있습니다.  
https://github.com/powerumc/httpgen-rs/releases

기본 설정 파일 생성:

```bash
./httpgen init
```

실행:

```bash
./httpgen run -i <interface> --eps 100 --vu 10
```


## Config

```yaml
test:
  vu: 10

http:
  req:
    headers:
      :host:
        # 배열은 값, 확률 가중치
        - ["{host}", 1]
      :path:
        - ["http://{host}/api/search?q={search_param}", 0.5]
        - ["http://{host}/login", 0.5]
      :method:
        # 0.5 확률로 GET 또는 POST값을 반환
        - ["GET", 0.5]
        - ["POST", 0.5]
      Connection: close
    body:
      # 가중치가 0인 경우 fallback 값으로 반환
      - ["{\"hello\": \"world\"}", 0]

params:
  host:
    - ["example.local", 0.5]
    - ["hello.local", 0.5]

src:
  cidr: 192.168.0.0/16
  port: 10000-50000
dst:
  cidr: 172.1.2.3/32
  port: 80-80
```
