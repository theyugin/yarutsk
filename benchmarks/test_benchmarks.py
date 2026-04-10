"""Performance benchmarks for yarutsk YAML library.

Usage:
    pip install pytest-benchmark
    pytest benchmarks/test_benchmarks.py --benchmark-min-rounds=10
"""

import io
import os
import sys
from typing import Any

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import pytest

yarutsk: Any = None
pyyaml: Any = None
RuamelYAML: Any = None

try:
    import yarutsk as _yarutsk

    yarutsk = _yarutsk
    HAS_YARUTSK = True
except ImportError:
    HAS_YARUTSK = False

try:
    import yaml as pyyaml

    HAS_PYYAML = True
except ImportError:
    HAS_PYYAML = False

try:
    from ruamel.yaml import YAML as _RuamelYAML  # type: ignore[import-not-found]

    RuamelYAML = _RuamelYAML
    HAS_RUAMEL = True
except ImportError:
    HAS_RUAMEL = False

SMALL_YAML = """
name: myapp
version: 1.0.0
description: A test application
debug: false
port: 8080
"""

MEDIUM_YAML = """
apiVersion: v1
kind: Service
metadata:
  name: my-service
  namespace: default
  labels:
    app: myapp
    environment: production
    version: v1.0.0
spec:
  type: ClusterIP
  ports:
    - port: 80
      targetPort: 8080
      protocol: TCP
      name: http
    - port: 443
      targetPort: 8443
      protocol: TCP
      name: https
  selector:
    app: myapp
    environment: production
"""

LARGE_YAML = """
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-deployment
  namespace: default
  labels:
    app: myapp
    version: v1.0.0
spec:
  replicas: 3
  selector:
    matchLabels:
      app: myapp
  template:
    metadata:
      labels:
        app: myapp
        version: v1.0.0
    spec:
      containers:
        - name: app
          image: myapp:latest
          ports:
            - containerPort: 8080
          env:
            - name: DATABASE_URL
              value: postgres://localhost:5432/mydb
            - name: REDIS_URL
              value: redis://localhost:6379
            - name: DEBUG
              value: "false"
            - name: LOG_LEVEL
              value: info
          resources:
            requests:
              memory: "128Mi"
              cpu: "100m"
            limits:
              memory: "256Mi"
              cpu: "200m"
          volumeMounts:
            - name: config
              mountPath: /etc/config
            - name: secrets
              mountPath: /etc/secrets
      volumes:
        - name: config
          configMap:
            name: app-config
        - name: secrets
          secret:
            secretName: app-secrets
"""

COMMENT_HEAVY_YAML = """
# Application Configuration
# This file contains all configuration options for the application

# General settings
name: myapp  # Application name
version: 1.0.0  # Semantic version

# Server configuration
server:
  # HTTP settings
  host: 0.0.0.0  # Bind address
  port: 8080  # Listen port
  # SSL/TLS settings
  ssl:
    enabled: false  # Enable HTTPS
    cert: /path/to/cert.pem  # Certificate file
    key: /path/to/key.pem  # Private key file

# Database settings
database:
  # Connection settings
  host: localhost  # Database host
  port: 5432  # Database port
  name: mydb  # Database name
  # Credentials
  user: admin  # Database user
  password: secret  # Database password

# Cache settings
cache:
  # Redis configuration
  redis:
    host: localhost  # Redis host
    port: 6379  # Redis port
    db: 0  # Redis database number
"""

NO_COMMENTS_YAML = """
name: myapp
version: 1.0.0
server:
  host: 0.0.0.0
  port: 8080
database:
  host: localhost
  port: 5432
cache:
  redis:
    host: localhost
    port: 6379
"""


@pytest.fixture(
    params=[
        ("small", SMALL_YAML),
        ("medium", MEDIUM_YAML),
        ("large", LARGE_YAML),
    ]
)
def yaml_document(request):
    """Provide YAML documents of various sizes."""
    return request.param


@pytest.mark.skipif(not HAS_YARUTSK, reason="yarutsk not built")
class TestYarutskBenchmarks:
    """Benchmarks for yarutsk."""

    def test_parse_small(self, benchmark):
        """Benchmark parsing small YAML."""

        def parse():
            return yarutsk.load(io.StringIO(SMALL_YAML))

        result = benchmark(parse)
        assert result["name"] == "myapp"

    def test_parse_medium(self, benchmark):
        """Benchmark parsing medium YAML."""

        def parse():
            return yarutsk.load(io.StringIO(MEDIUM_YAML))

        result = benchmark(parse)
        assert result["metadata"]["name"] == "my-service"

    def test_parse_large(self, benchmark):
        """Benchmark parsing large YAML."""

        def parse():
            return yarutsk.load(io.StringIO(LARGE_YAML))

        result = benchmark(parse)
        assert result["metadata"]["name"] == "my-deployment"

    def test_serialize_small(self, benchmark):
        """Benchmark serializing small YAML."""
        doc = yarutsk.load(io.StringIO(SMALL_YAML))

        def serialize():
            output = io.StringIO()
            doc.dump(output)
            return output.getvalue()

        result = benchmark(serialize)
        assert "name: myapp" in result

    def test_serialize_medium(self, benchmark):
        """Benchmark serializing medium YAML."""
        doc = yarutsk.load(io.StringIO(MEDIUM_YAML))

        def serialize():
            output = io.StringIO()
            doc.dump(output)
            return output.getvalue()

        result = benchmark(serialize)
        assert "my-service" in result

    def test_serialize_large(self, benchmark):
        """Benchmark serializing large YAML."""
        doc = yarutsk.load(io.StringIO(LARGE_YAML))

        def serialize():
            output = io.StringIO()
            doc.dump(output)
            return output.getvalue()

        result = benchmark(serialize)
        assert "my-deployment" in result

    def test_comment_heavy_parse(self, benchmark):
        """Benchmark parsing comment-heavy YAML."""

        def parse():
            return yarutsk.load(io.StringIO(COMMENT_HEAVY_YAML))

        result = benchmark(parse)
        assert result["name"] == "myapp"

    def test_comment_heavy_serialize(self, benchmark):
        """Benchmark serializing comment-heavy YAML."""
        doc = yarutsk.load(io.StringIO(COMMENT_HEAVY_YAML))

        def serialize():
            output = io.StringIO()
            doc.dump(output)
            return output.getvalue()

        result = benchmark(serialize)
        assert "# Application Configuration" in result

    def test_key_access(self, benchmark):
        """Benchmark key access."""
        doc = yarutsk.load(io.StringIO(LARGE_YAML))

        def access():
            return doc["metadata"]["name"]

        result = benchmark(access)
        assert result == "my-deployment"

    def test_nested_access(self, benchmark):
        """Benchmark nested key access."""
        doc = yarutsk.load(io.StringIO(LARGE_YAML))

        def access():
            return doc["spec"]["template"]["spec"]["containers"][0]["name"]

        result = benchmark(access)
        assert result == "app"


@pytest.mark.skipif(not HAS_PYYAML, reason="PyYAML not installed")
class TestPyYAMLComparison:
    """Comparison benchmarks with PyYAML."""

    def test_pyyaml_parse_small(self, benchmark):
        """Benchmark PyYAML parsing small YAML."""

        def parse():
            return pyyaml.safe_load(SMALL_YAML)

        result = benchmark(parse)
        assert result["name"] == "myapp"

    def test_pyyaml_parse_medium(self, benchmark):
        """Benchmark PyYAML parsing medium YAML."""

        def parse():
            return pyyaml.safe_load(MEDIUM_YAML)

        result = benchmark(parse)
        assert result["metadata"]["name"] == "my-service"

    def test_pyyaml_parse_large(self, benchmark):
        """Benchmark PyYAML parsing large YAML."""

        def parse():
            return pyyaml.safe_load(LARGE_YAML)

        result = benchmark(parse)
        assert result["metadata"]["name"] == "my-deployment"

    def test_pyyaml_serialize_small(self, benchmark):
        """Benchmark PyYAML serializing small YAML."""
        doc = pyyaml.safe_load(SMALL_YAML)

        def serialize():
            return pyyaml.dump(doc)

        result = benchmark(serialize)
        assert "name: myapp" in result


@pytest.mark.skipif(not HAS_RUAMEL, reason="ruamel.yaml not installed")
class TestRuamelComparison:
    """Comparison benchmarks with ruamel.yaml."""

    def test_ruamel_parse_small(self, benchmark):
        """Benchmark ruamel.yaml parsing small YAML."""
        yaml = RuamelYAML()

        def parse():
            return yaml.load(SMALL_YAML)

        result = benchmark(parse)
        assert result["name"] == "myapp"

    def test_ruamel_parse_medium(self, benchmark):
        """Benchmark ruamel.yaml parsing medium YAML."""
        yaml = RuamelYAML()

        def parse():
            return yaml.load(MEDIUM_YAML)

        result = benchmark(parse)
        assert result["metadata"]["name"] == "my-service"

    def test_ruamel_serialize_small(self, benchmark):
        """Benchmark ruamel.yaml serializing small YAML."""
        yaml = RuamelYAML()
        doc = yaml.load(SMALL_YAML)

        def serialize():
            stream = io.StringIO()
            yaml.dump(doc, stream)
            return stream.getvalue()

        result = benchmark(serialize)
        assert "name: myapp" in result


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--benchmark-min-rounds=10"])
