/**
 * Docker Compose parser unit tests.
 *
 * Tests the adapter layer YAML parsing and DTO construction.
 */

import { describe, expect, it } from "vitest";
import {
	isComposeFileName,
	parseComposeFile,
} from "../../../src/adapters/compose/compose-parser.js";

describe("isComposeFileName", () => {
	it("matches docker-compose.yml", () => {
		expect(isComposeFileName("docker-compose.yml")).toBe(true);
	});

	it("matches docker-compose.yaml", () => {
		expect(isComposeFileName("docker-compose.yaml")).toBe(true);
	});

	it("matches compose.yml", () => {
		expect(isComposeFileName("compose.yml")).toBe(true);
	});

	it("matches compose.yaml", () => {
		expect(isComposeFileName("compose.yaml")).toBe(true);
	});

	it("is case-insensitive", () => {
		expect(isComposeFileName("Docker-Compose.yml")).toBe(true);
		expect(isComposeFileName("DOCKER-COMPOSE.YML")).toBe(true);
	});

	it("rejects non-compose files", () => {
		expect(isComposeFileName("package.json")).toBe(false);
		expect(isComposeFileName("Dockerfile")).toBe(false);
		expect(isComposeFileName("docker-compose.json")).toBe(false);
	});
});

describe("parseComposeFile", () => {
	it("parses simple compose file with services", () => {
		const content = `
version: "3.8"
services:
  web:
    image: nginx:latest
    ports:
      - "80:80"
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result).not.toBeNull();
		expect(result!.version).toBe("3.8");
		expect(result!.filePath).toBe("docker-compose.yml");
		expect(result!.services).toHaveLength(1);
		expect(result!.services[0].name).toBe("web");
		expect(result!.services[0].image).toBe("nginx:latest");
		expect(result!.services[0].ports).toEqual(["80:80"]);
	});

	it("parses compose file without version (v3+ optional)", () => {
		const content = `
services:
  app:
    image: node:20
`;
		const result = parseComposeFile(content, "compose.yml");
		expect(result).not.toBeNull();
		expect(result!.version).toBeNull();
		expect(result!.services).toHaveLength(1);
	});

	it("parses build configuration - short form", () => {
		const content = `
services:
  api:
    build: ./backend
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result).not.toBeNull();
		expect(result!.services[0].build).not.toBeNull();
		expect(result!.services[0].build!.context).toBe("./backend");
		expect(result!.services[0].build!.dockerfile).toBeNull();
	});

	it("parses build configuration - long form", () => {
		const content = `
services:
  api:
    build:
      context: ./services/api
      dockerfile: Dockerfile.prod
      target: production
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result).not.toBeNull();
		expect(result!.services[0].build).not.toBeNull();
		expect(result!.services[0].build!.context).toBe("./services/api");
		expect(result!.services[0].build!.dockerfile).toBe("Dockerfile.prod");
		expect(result!.services[0].build!.target).toBe("production");
	});

	it("parses command as array", () => {
		const content = `
services:
  app:
    image: node:20
    command: ["npm", "start"]
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result!.services[0].command).toEqual(["npm", "start"]);
	});

	it("parses command as string (shell form)", () => {
		const content = `
services:
  app:
    image: node:20
    command: npm start
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result!.services[0].command).toEqual(["npm", "start"]);
	});

	it("parses entrypoint", () => {
		const content = `
services:
  app:
    image: python:3.11
    entrypoint: ["python", "server.py"]
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result!.services[0].entrypoint).toEqual(["python", "server.py"]);
	});

	it("parses environment variables - array form", () => {
		const content = `
services:
  app:
    image: node:20
    environment:
      - NODE_ENV=production
      - PORT=3000
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result!.services[0].envVars).toEqual(["NODE_ENV", "PORT"]);
	});

	it("parses environment variables - object form", () => {
		const content = `
services:
  app:
    image: node:20
    environment:
      NODE_ENV: production
      PORT: 3000
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result!.services[0].envVars).toContain("NODE_ENV");
		expect(result!.services[0].envVars).toContain("PORT");
	});

	it("parses depends_on - simple form", () => {
		const content = `
services:
  api:
    image: node:20
    depends_on:
      - db
      - redis
  db:
    image: postgres:15
  redis:
    image: redis:7
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		const api = result!.services.find((s) => s.name === "api");
		expect(api!.dependsOn).toEqual(["db", "redis"]);
	});

	it("parses depends_on - extended form", () => {
		const content = `
services:
  api:
    image: node:20
    depends_on:
      db:
        condition: service_healthy
      redis:
        condition: service_started
  db:
    image: postgres:15
  redis:
    image: redis:7
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		const api = result!.services.find((s) => s.name === "api");
		expect(api!.dependsOn).toContain("db");
		expect(api!.dependsOn).toContain("redis");
	});

	it("parses profiles", () => {
		const content = `
services:
  debug:
    image: busybox
    profiles:
      - debug
      - dev
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result!.services[0].profiles).toEqual(["debug", "dev"]);
	});

	it("parses container_name", () => {
		const content = `
services:
  app:
    image: node:20
    container_name: my-custom-container
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result!.services[0].containerName).toBe("my-custom-container");
	});

	it("parses multiple services", () => {
		const content = `
services:
  api:
    build: .
  worker:
    build:
      context: .
      dockerfile: Dockerfile.worker
  redis:
    image: redis:7-alpine
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result!.services).toHaveLength(3);
		expect(result!.services.map((s) => s.name).sort()).toEqual(["api", "redis", "worker"]);
	});

	it("returns null for invalid YAML", () => {
		const content = `
services:
  api
    - invalid yaml
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result).toBeNull();
	});

	it("returns null for YAML without services", () => {
		const content = `
version: "3.8"
networks:
  default:
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result).toBeNull();
	});

	it("returns null for empty services", () => {
		const content = `
version: "3.8"
services: {}
`;
		const result = parseComposeFile(content, "docker-compose.yml");
		expect(result).toBeNull();
	});
});
