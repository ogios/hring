# Maintainer: Xhelgi <helgi@proton.me>
pkgname=hring
pkgver=0.2.0.r22.g33b1aa0
pkgrel=1
pkgdesc='Lightweight keyboard-driven application launcher with a graph-like interface'
arch=('x86_64' 'aarch64')
url='https://github.com/Xhelgi/hring'
license=('GPL-3.0-only')
depends=('gcc-libs')
makedepends=('cargo' 'rust')
options=('!debug')
source=()
sha256sums=()

# Derive pkgver from git so every commit produces a new, rebuild-forcing version.
# Falls back to Cargo.toml version + commit count/hash when no tags exist.
pkgver() {
  cd "$startdir"
  local desc base count
  if desc=$(git describe --long --tags --abbrev=7 2>/dev/null); then
    printf '%s' "$desc" | sed 's/^v//; s/\([^-]*-g\)/r\1/; s/-/./g'
  else
    base=$(grep -m1 '^version = ' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
    count=$(git rev-list --count HEAD)
    printf '%s.r%s.g%s' "${base:-0.2.0}" "$count" "$(git rev-parse --short=7 HEAD)"
  fi
}

build() {
  cd "$startdir"
  cargo build --release --locked
}

package() {
  cd "$startdir"

  install -Dm755 target/release/hring "$pkgdir/usr/bin/hring"
  install -Dm644 packaging/hring.desktop "$pkgdir/usr/share/applications/hring.desktop"
  install -Dm644 packaging/hring.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/hring.svg"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
