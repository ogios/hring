# Maintainer: Xhelgi <helgi@proton.me>
pkgname=hring
pkgver=0.2.0
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
