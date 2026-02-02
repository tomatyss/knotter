class Knotter < Formula
  desc "Terminal-first personal CRM and friendship tracker"
  homepage "https://github.com/tomatyss/knotter"
  url "https://github.com/tomatyss/knotter/archive/refs/tags/v0.4.4.tar.gz"
  sha256 "af02196b6e6100f508230ebf266fc19d501b7b10228a7e19d37efadd46d2275e"
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
