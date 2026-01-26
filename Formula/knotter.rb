class Knotter < Formula
  desc "Terminal-first personal CRM and friendship tracker"
  homepage "https://github.com/tomatyss/knotter"
  url "https://github.com/tomatyss/knotter/archive/refs/tags/v0.2.0.tar.gz"
  sha256 "46a198508d36e4ebbb352e8a4e2b00a3c68729644eeb16256d95176cd5e8d9e9"
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
