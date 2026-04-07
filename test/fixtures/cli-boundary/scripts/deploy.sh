#!/bin/bash
set -e

# Build
tsc && vitest run

# Deploy
mytool repo add staging
mytool build
