# Fork-only release helpers for agentx. Upstream codex has no top-level Makefile.

.PHONY: agentx-release agentx-release-prerelease

# Cut an agentx release. Bumps Cargo.toml workspace.package.version, refreshes
# Cargo.lock, commits, and tags.
#
# Usage: make agentx-release VERSION=0.128.0-agentx.1
#
# After this target completes, push to remote:
#   git push origin main && git push origin agentx-v$(VERSION)
agentx-release:
	@test -n "$(VERSION)" || (echo "VERSION=x.y.z[-agentx.N] required"; exit 1)
	@echo "$(VERSION)" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-agentx\.[0-9]+)?$$' \
		|| (echo "VERSION must match x.y.z or x.y.z-agentx.N (got: $(VERSION))"; exit 1)
	sed -i 's/^version = .*/version = "$(VERSION)"/' codex-rs/Cargo.toml
	cd codex-rs && cargo update --workspace --quiet
	git commit -am "chore(release): agentx $(VERSION)"
	git tag agentx-v$(VERSION)
	@echo
	@echo "Tagged agentx-v$(VERSION). Now run:"
	@echo "  git push origin main && git push origin agentx-v$(VERSION)"

# Convenience for the very first dry-run release.
agentx-release-prerelease:
	$(MAKE) agentx-release VERSION=0.128.0-agentx.0
