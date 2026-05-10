# httpgen

`httpgen`은 NIC에 raw Ethernet/TCP/HTTP 패킷을 직접 생성해 전송하는 HTTP traffic generator입니다.

## Usage

기본 설정 파일 생성:

```bash
cargo run -- init
```

실행:

```bash
sudo cargo run -- run -i <interface> -c httpgen.config.yaml --eps 1000
```

`-c/--config-file`을 생략하면 빌드에 포함된 기본 `httpgen.config.yaml`을 사용합니다.
raw datalink 송신 권한이 필요하므로 일반적으로 `sudo`가 필요합니다.

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
