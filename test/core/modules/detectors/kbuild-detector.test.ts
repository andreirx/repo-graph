/**
 * Unit tests for Kbuild module detector.
 *
 * Pure function tests — no filesystem, no storage. Tests cover:
 *   - obj-y directory assignments
 *   - obj-m directory assignments
 *   - obj-$(CONFIG_...) config-gated assignments (D1: skipped with diagnostic)
 *   - Line continuation handling
 *   - Nested Makefile path resolution
 *   - Skipped conditionals (diagnostics)
 *   - Skipped variable references (diagnostics)
 *   - isKbuildFile helper
 */

import { describe, expect, it } from "vitest";
import {
	detectKbuildModules,
	isKbuildFile,
} from "../../../../src/core/modules/detectors/kbuild-detector.js";

// ── Basic obj-y detection ──────────────────────────────────────────

describe("detectKbuildModules — obj-y", () => {
	it("detects single obj-y directory assignment", () => {
		const content = `
# Linux kernel drivers Makefile
obj-y += cache/
`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("drivers/cache");
		expect(result.modules[0].displayName).toBe("cache");
		expect(result.modules[0].sourceType).toBe("kbuild");
		expect(result.modules[0].evidenceKind).toBe("kbuild_subdir");
		expect(result.modules[0].confidence).toBe(0.9);
	});

	it("detects multiple obj-y directories on same line", () => {
		const content = `obj-y += cache/ irqchip/ bus/`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		expect(result.modules).toHaveLength(3);
		expect(result.modules.map((m) => m.rootPath)).toEqual([
			"drivers/cache",
			"drivers/irqchip",
			"drivers/bus",
		]);
	});

	it("detects obj-y with := assignment", () => {
		const content = `obj-y := core/`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("core");
	});

	it("ignores obj-y object files (not directories)", () => {
		const content = `
obj-y += fork.o exec_domain.o
obj-y += kernel/
`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("kernel");
	});

	it("handles root-level Makefile correctly", () => {
		const content = `obj-y += init/`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules[0].rootPath).toBe("init");
	});
});

// ── obj-m detection ────────────────────────────────────────────────

describe("detectKbuildModules — obj-m", () => {
	it("detects obj-m directory assignment", () => {
		const content = `obj-m += mymodule/`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("drivers/mymodule");
		const payload = result.modules[0].payload as { assignmentType: string };
		expect(payload.assignmentType).toBe("obj-m");
	});
});

// ── obj-$(CONFIG_...) skipping (D1: out of scope) ──────────────────

describe("detectKbuildModules — config-gated", () => {
	it("skips obj-$(CONFIG_...) assignments with diagnostic", () => {
		const content = `obj-$(CONFIG_ACPI) += acpi/`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		// D1 scope: obj-$(CONFIG_...) is config-gated, must be skipped
		expect(result.modules).toHaveLength(0);
		expect(result.diagnostics.some((d) => d.kind === "skipped_config_gated")).toBe(true);
	});

	it("skips multiple CONFIG patterns with diagnostics", () => {
		const content = `
obj-$(CONFIG_PCI) += pci/
obj-$(CONFIG_USB) += usb/
obj-$(CONFIG_NET) += net/
`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		expect(result.modules).toHaveLength(0);
		expect(result.diagnostics.filter((d) => d.kind === "skipped_config_gated")).toHaveLength(3);
	});

	it("detects obj-y but skips obj-$(CONFIG_...) in same file", () => {
		const content = `
obj-y += always/
obj-$(CONFIG_FOO) += conditional/
`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules).toHaveLength(1);
		expect(result.modules[0].rootPath).toBe("always");
		expect(result.diagnostics.some((d) => d.kind === "skipped_config_gated")).toBe(true);
	});
});

// ── Line continuation ──────────────────────────────────────────────

describe("detectKbuildModules — line continuation", () => {
	it("handles backslash line continuation", () => {
		const content = `
obj-y += cache/ \\
         irqchip/ \\
         bus/
`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		expect(result.modules).toHaveLength(3);
		expect(result.modules.map((m) => m.displayName)).toEqual([
			"cache",
			"irqchip",
			"bus",
		]);
	});
});

// ── Deduplication ──────────────────────────────────────────────────

describe("detectKbuildModules — deduplication", () => {
	it("deduplicates repeated obj-y directory references", () => {
		const content = `
obj-y += core/
obj-y += core/
`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules).toHaveLength(1);
	});

	it("obj-y emits module even if same path appears in skipped CONFIG line", () => {
		// obj-$(CONFIG_FOO) is skipped, obj-y is detected
		const content = `
obj-y += core/
obj-$(CONFIG_FOO) += core/
`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules).toHaveLength(1);
		expect(result.diagnostics.some((d) => d.kind === "skipped_config_gated")).toBe(true);
	});
});

// ── Nested paths ───────────────────────────────────────────────────

describe("detectKbuildModules — nested paths", () => {
	it("resolves paths relative to Makefile directory", () => {
		const content = `obj-y += keys/`;
		const result = detectKbuildModules(content, "crypto/asymmetric_keys/Makefile");
		expect(result.modules[0].rootPath).toBe("crypto/asymmetric_keys/keys");
	});

	it("handles char/ipmi style nested paths", () => {
		const content = `obj-y += char/ipmi/`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		expect(result.modules[0].rootPath).toBe("drivers/char/ipmi");
	});
});

// ── Diagnostics ────────────────────────────────────────────────────

describe("detectKbuildModules — diagnostics", () => {
	it("skips assignments inside conditional blocks", () => {
		const content = `
ifeq ($(CONFIG_FOO),y)
obj-y += foo/
endif
`;
		const result = detectKbuildModules(content, "Makefile");
		// foo/ must NOT be detected — it's inside a conditional block (D1: out of scope)
		expect(result.modules).toHaveLength(0);
		// Record diagnostics for the conditional structure
		expect(result.diagnostics.some((d) => d.kind === "skipped_conditional")).toBe(
			true,
		);
		// Record diagnostic for the skipped assignment
		expect(
			result.diagnostics.some((d) => d.kind === "skipped_inside_conditional"),
		).toBe(true);
	});

	it("tracks nested conditional depth correctly", () => {
		const content = `
obj-y += before/
ifeq ($(CONFIG_FOO),y)
  ifdef CONFIG_BAR
    obj-y += nested/
  endif
  obj-y += inner/
endif
obj-y += after/
`;
		const result = detectKbuildModules(content, "Makefile");
		// Only before/ and after/ should be detected (outside conditionals)
		expect(result.modules).toHaveLength(2);
		expect(result.modules.map((m) => m.displayName)).toEqual(["before", "after"]);
		// nested/ and inner/ skipped
		expect(
			result.diagnostics.filter((d) => d.kind === "skipped_inside_conditional"),
		).toHaveLength(2);
	});

	it("records skipped variable references", () => {
		const content = `obj-y += $(my-subdirs)/`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules).toHaveLength(0);
		expect(result.diagnostics.some((d) => d.kind === "skipped_variable")).toBe(
			true,
		);
	});
});

// ── Edge cases ─────────────────────────────────────────────────────

describe("detectKbuildModules — edge cases", () => {
	it("handles empty content", () => {
		const result = detectKbuildModules("", "Makefile");
		expect(result.modules).toHaveLength(0);
		expect(result.diagnostics).toHaveLength(0);
	});

	it("handles comment-only content", () => {
		const content = `
# This is a comment
# Another comment
`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules).toHaveLength(0);
	});

	it("ignores other Makefile constructs", () => {
		const content = `
CFLAGS += -Wall
include scripts/Makefile.lib
all: vmlinux
`;
		const result = detectKbuildModules(content, "Makefile");
		expect(result.modules).toHaveLength(0);
	});

	it("handles whitespace variations", () => {
		const content = `
obj-y		+= cache/
obj-y+=irqchip/
obj-y  :=  bus/
`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		expect(result.modules).toHaveLength(3);
	});
});

// ── Real Linux kernel snippets ─────────────────────────────────────

describe("detectKbuildModules — real kernel patterns", () => {
	it("parses drivers/Makefile style (obj-y only, CONFIG skipped)", () => {
		const content = `
# SPDX-License-Identifier: GPL-2.0
obj-y				+= cache/
obj-y				+= irqchip/
obj-y				+= bus/
obj-y				+= phy/
obj-$(CONFIG_PINCTRL)		+= pinctrl/
obj-$(CONFIG_GPIOLIB)		+= gpio/
obj-y				+= pwm/
`;
		const result = detectKbuildModules(content, "drivers/Makefile");
		// D1 scope: only obj-y modules detected (5), obj-$(CONFIG_...) skipped (2)
		expect(result.modules).toHaveLength(5);
		expect(result.modules.map((m) => m.displayName)).toEqual([
			"cache", "irqchip", "bus", "phy", "pwm",
		]);
		// CONFIG-gated lines recorded as diagnostics
		expect(result.diagnostics.filter((d) => d.kind === "skipped_config_gated")).toHaveLength(2);
	});

	it("parses crypto/Makefile style with CONFIG object files", () => {
		const content = `
obj-$(CONFIG_CRYPTO) += crypto.o
crypto-y := api.o cipher.o
obj-$(CONFIG_CRYPTO_ALGAPI2) += crypto_algapi.o
obj-$(CONFIG_CRYPTO_AEAD2) += aead.o
`;
		const result = detectKbuildModules(content, "crypto/Makefile");
		// No directory assignments, all are .o object files
		// CONFIG lines are skipped (D1: out of scope)
		expect(result.modules).toHaveLength(0);
		// All obj-$(CONFIG_...) lines recorded as skipped
		expect(result.diagnostics.filter((d) => d.kind === "skipped_config_gated")).toHaveLength(3);
	});
});

// ── isKbuildFile helper ────────────────────────────────────────────

describe("isKbuildFile", () => {
	it("returns true for Makefile", () => {
		expect(isKbuildFile("Makefile")).toBe(true);
		expect(isKbuildFile("drivers/Makefile")).toBe(true);
		expect(isKbuildFile("arch/x86/kernel/Makefile")).toBe(true);
	});

	it("returns true for Kbuild", () => {
		expect(isKbuildFile("Kbuild")).toBe(true);
		expect(isKbuildFile("drivers/Kbuild")).toBe(true);
	});

	it("returns false for Makefile.am", () => {
		expect(isKbuildFile("Makefile.am")).toBe(false);
	});

	it("returns false for .mk includes", () => {
		expect(isKbuildFile("rules.mk")).toBe(false);
		expect(isKbuildFile("scripts/Makefile.lib")).toBe(false);
	});

	it("returns false for other files", () => {
		expect(isKbuildFile("main.c")).toBe(false);
		expect(isKbuildFile("README")).toBe(false);
	});
});
