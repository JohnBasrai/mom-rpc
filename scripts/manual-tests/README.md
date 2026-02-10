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

### RabbitMQ (AMQP)

Tests AMQP transport implementations against a RabbitMQ broker.

**Usage:**
```bash
./rabbitmq.sh transport_lapin
```

**Requirements:**
- Docker installed and running
- Ports 5672 (AMQP) and 15672 (management UI) available

**What it does:**
1. Starts RabbitMQ in Docker
2. Builds math_server and math_client examples with specified feature
3. Runs server in background
4. Runs client and validates output contains "2 + 3 = 5"
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
2. Builds math_server and math_client examples with specified feature
3. Runs server in background
4. Runs client and validates output contains "2 + 3 = 5"
5. Cleans up (kills server, stops container)

## Design Principles

### Organized by Broker, Not Library

Scripts are organized by **broker type** because:
- Multiple libraries may implement the same protocol
- Test logic is the same regardless of which library is used
- Reduces duplication

For example:
- `rabbitmq.sh` can test `transport_lapin` or any future AMQP library
- `mqtt.sh` can test `transport_rumqttc`, `transport_paho`, etc.

### Feature Flag as Parameter

Each script takes a feature flag as a parameter:
```bash
./rabbitmq.sh transport_lapin          # Test lapin
./rabbitmq.sh transport_another_amqp   # Future: test another AMQP lib
./mqtt.sh transport_rumqttc            # Test rumqttc
./mqtt.sh transport_paho               # Future: test paho
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
pkill -f math_server

# Remove containers
docker rm -f mom-rpc-test-rabbitmq
docker rm -f mom-rpc-test-mosquitto
```

## Adding New Tests

To add a test for a new broker type:

1. Create `scripts/manual-tests/broker_name.sh`
2. Follow the pattern from existing scripts:
   - Accept feature flag as `$1`
   - Check prerequisites (Docker, cargo)
   - Start broker in Docker
   - Build and run examples
   - Validate output
   - Clean up via trap
3. Make executable: `chmod +x broker_name.sh`
4. Update this README

## Notes

- These tests assume examples read `BROKER_URI` from environment
- Examples should be updated to support different transports via env vars
- Tests use `--quiet` flag to reduce cargo output noise
- Broker logs available via: `docker logs <container-name>`
