class Knotter < Formula
  desc "Terminal-first personal CRM and friendship tracker"
  homepage "https://github.com/tomatyss/knotter"
  url "https://github.com/tomatyss/knotter/archive/refs/tags/v4.1.0.tar.gz"
  sha256 "eebe801036ccf763021035e38ad1d92efbc1de8c2c353b144abdb00821cdf4e3"
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
