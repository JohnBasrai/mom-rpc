# Manual Integration Tests

This directory contains manual integration test scripts for validating transport implementations against real brokers.

## Purpose

These scripts are **not part of CI**. They are for:
- Local development and testing
- Validating transport implementations work with real brokers
- Quick smoke testing before releases
- Debugging transport-specific issues

CI continues to use the fast, reliable memory transport for automated testing.

## Available Tests

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
2. Builds sensor_server and sensor_client examples with specified feature
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
2. Builds sensor_server and sensor_client examples with specified feature
3. Runs server in background
4. Runs client and validates output contains "Temperature", "Humidity", and "Pressure"
5. Cleans up (kills server, stops container)

### DDS

Tests DDS transport implementations. DDS is brokerless - peers discover each other automatically via RTPS multicast.

**Usage:**
```bash
./dds.sh transport_dust_dds
```

**Optional - preserve logs:**
```bash
./dds.sh transport_dust_dds NO_CLEAN
```

**Requirements:**
- No external infrastructure (brokerless)
- Multicast-enabled network interface

**What it does:**
1. Builds sensor_server and sensor_client examples with specified feature
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

### Feature Flag as Parameter

Each script takes a feature flag as a parameter:
```bash
./amqp.sh transport_lapin          # Test lapin
./amqp.sh transport_another_amqp   # Future: test another AMQP lib
./mqtt.sh transport_rumqttc        # Test rumqttc
./mqtt.sh transport_paho           # Future: test paho
```

This allows testing multiple implementations without duplicating test logic.

## Exit Codes

- `0` - Test passed
- `1` - Test failed or error occurred

## Cleanup

All scripts use `trap` to ensure cleanup happens even on failure:
- Server processes are killed
- Docker containers are stopped and removed
- Temporary files are deleted

You can also manually clean up:
```bash
# Kill any lingering processes
pkill -f sensor_server

# Remove containers
docker rm -f mom-rpc-test-rabbitmq
docker rm -f mom-rpc-test-mosquitto
```

## Adding New Tests

To add a test for a new protocol type:

1. Create `scripts/manual-tests/<protocol-name>.sh`
2. Follow the pattern from existing scripts:
   - Accept feature flag as `$1`
   - Check prerequisites (Docker, cargo)
   - Start broker in Docker
   - Build and run examples
   - Validate output
   - Clean up via trap
3. Make executable: `chmod +x <protocol-name>.sh`
4. Update this README

## Notes

- These tests assume examples read `BROKER_URI` from environment
- Examples should be updated to support different transports via env vars
- Tests use `--quiet` flag to reduce cargo output noise
- Broker logs available via: `docker logs <container-name>`
