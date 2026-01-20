class Knotter < Formula
  desc "Terminal-first personal CRM and friendship tracker"
  homepage "https://github.com/tomatyss/knotter"
  url "https://github.com/tomatyss/knotter/archive/refs/tags/v0.1.1.tar.gz"
  sha256 "1e2b0589d90c8b13d0d74f8b90b656877bc620ead0bbf7c9a29448d262eb1a89"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "-p", "knotter-cli", "-p", "knotter-tui"
    bin.install "target/release/knotter"
    bin.install "target/release/knotter-tui"
  end

  test do
    system "#{bin}/knotter", "--help"
    system "#{bin}/knotter-tui", "--help"
  end
end
