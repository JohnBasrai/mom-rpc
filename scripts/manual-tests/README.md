# Manual Integration Tests

This directory contains manual integration test scripts for validating transport implementations against real brokers.

## Purpose

These scripts are **not part of CI**. They are for:
- Local development and testing
- Validating transport implementations work with real brokers
- Quick smoke testing before releases
- Debugging transport-specific issues

CI continues to use the fast, reliable memory transport for automated testing.

## Example Used: `sensor_fullduplex`

All broker-based scripts use the `sensor_fullduplex` example rather than the
`sensor_server` / `sensor_client` pair. This is intentional:

- **Full-duplex exercises both subscription directions** — the node subscribes
  to both its request topic and its response topic back-to-back, which is the
  primary stress case for transport-layer subscription serialization.
- **Self-contained** — a single process acts as both client and server, so
  there is no inter-process timing dependency and no background server to
  manage.
- **Transport validation focus** — the script validates the transport layer,
  not RPC logic. `sensor_fullduplex` produces clear, deterministic output
  (Temperature, Humidity, Pressure readings) that confirms end-to-end
  publish/subscribe works correctly.

The AMQP and MQTT scripts still use the `sensor_server` / `sensor_client`
pair because those transports predate this approach and validate the same
transport surface area. New transport scripts should use `sensor_fullduplex`.

## Available Tests

### Redis

Tests Redis Pub/Sub transport against a Redis broker.

**Usage:**
```bash
./redis.sh
```

**Requirements:**
- Docker installed and running
- Port 6379 (Redis) available

**What it does:**
1. Starts Redis in Docker
2. Builds `sensor_fullduplex` with `transport_redis` feature
3. Runs the example and validates output contains "Temperature", "Humidity", and "Pressure"
4. Cleans up (stops container)

### AMQP

Tests AMQP transport implementations against a RabbitMQ broker.

**Usage:**
```bash
./amqp.sh transport_lapin
```

**Requirements:**
- Docker installed and running
- Ports 5672 (AMQP) and 15672 (management UI) available

**What it does:**
1. Starts RabbitMQ in Docker
2. Builds `sensor_server` and `sensor_client` examples with specified feature
3. Runs server in background
4. Runs client and validates output contains "Temperature", "Humidity", and "Pressure"
5. Cleans up (kills server, stops container)

**Management UI:**
- URL: http://localhost:15672
- Username: `guest`
- Password: `guest`

### MQTT

Tests MQTT transport implementations against a Mosquitto broker.

**Usage:**
```bash
./mqtt.sh transport_rumqttc
```

**Requirements:**
- Docker installed and running
- Port 1883 (MQTT) available

**What it does:**
1. Starts Mosquitto in Docker
2. Builds `sensor_server` and `sensor_client` examples with specified feature
3. Runs server in background
4. Runs client and validates output contains "Temperature", "Humidity", and "Pressure"
5. Cleans up (kills server, stops container)

### DDS

Tests DDS transport implementations. DDS is brokerless — peers discover each other automatically via RTPS multicast.

**Usage:**
```bash
./dds.sh transport_dust_dds
```

**Optional — preserve logs:**
```bash
./dds.sh transport_dust_dds NO_CLEAN
```

**Requirements:**
- No external infrastructure (brokerless)
- Multicast-enabled network interface

**What it does:**
1. Builds `sensor_server` and `sensor_client` examples with specified feature
2. Runs server in background
3. Waits for DDS discovery (default: 1 second)
4. Runs client and validates output contains "Temperature", "Humidity", and "Pressure"
5. Cleans up (kills server, removes logs unless NO_CLEAN specified)

**Environment variables:**
- `DOMAIN_ID` - DDS domain ID (default: 0)
- `TRANSPORT_URI` - Transport URI (default: `dds:${DOMAIN_ID}`)

**Discovery:**
DDS uses RTPS multicast for automatic peer discovery. No broker configuration needed.

**Troubleshooting:**
- If discovery fails, check multicast is enabled on your network interface
- Firewall rules may block multicast packets
- Use `NO_CLEAN` parameter to preserve server.log and client.log for debugging


## Design Principles

### Organized by Protocol, Not Library

Scripts are organized by **protocol type** because:
- Multiple libraries may implement the same protocol
- Test logic is the same regardless of which library is used
- Reduces duplication

For example:
- `amqp.sh` can test `transport_lapin` or any future AMQP library
- `mqtt.sh` can test `transport_rumqttc`, `transport_paho`, etc.

### Feature Flag as Parameter (broker scripts)

AMQP and MQTT scripts take a feature flag as a parameter to allow testing
multiple library implementations without duplicating test logic:
```bash
./amqp.sh transport_lapin          # Test lapin
./amqp.sh transport_another_amqp   # Future: test another AMQP lib
./mqtt.sh transport_rumqttc        # Test rumqttc
./mqtt.sh transport_paho           # Future: test paho
```

Redis has a single implementation (`transport_redis`) so its script takes no
parameter.

## Exit Codes

- `0` - Test passed
- `1` - Test failed or error occurred

## Cleanup

All scripts use `trap` to ensure cleanup happens even on failure:
- Server processes are killed (where applicable)
- Docker containers are stopped and removed
- Temporary files are deleted

You can also manually clean up:
```bash
# Kill any lingering processes
pkill -f sensor_server

# Remove containers
docker rm -f mom-rpc-test-rabbitmq
docker rm -f mom-rpc-test-mosquitto
docker rm -f mom-rpc-test-redis
```

## Adding New Tests

To add a test for a new transport:

1. Create `scripts/manual-tests/<protocol-name>.sh`
2. Use `sensor_fullduplex` if the transport is new; follow the Redis script as
   the reference pattern.
3. Follow the pattern from existing scripts:
   - Check prerequisites (Docker, cargo)
   - Start broker in Docker (if applicable)
   - Build and run `sensor_fullduplex` (or server+client for legacy transports)
   - Validate output contains "Temperature", "Humidity", and "Pressure"
   - Clean up via `trap`
4. Make executable: `chmod +x <protocol-name>.sh`
5. Update this README

## Notes

- Scripts assume examples read `BROKER_URI` from the environment
- Tests use `--quiet` flag to reduce cargo output noise
- Broker logs available via: `docker logs <container-name>`
