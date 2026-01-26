class Knotter < Formula
  desc "Terminal-first personal CRM and friendship tracker"
  homepage "https://github.com/tomatyss/knotter"
  url "https://github.com/tomatyss/knotter/archive/refs/tags/v0.2.1.tar.gz"
  sha256 "cb4d16c140173d157991baece60d01a412db6625d1c55af354aaa6f27ae1dc81"
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
