class Vtcode < Formula
  desc "Rust-based terminal coding agent with semantic code intelligence"
  homepage "https://github.com/vinhnx/vtcode"
  license "MIT"
  version "0.166.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/vinhnx/vtcode/releases/download/#{version}/vtcode-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "abf3bfbd3ca5e432a0f5a7db1c2b8f25b884f346b97451bcb5f457066f749e8f"
    else
      url "https://github.com/vinhnx/vtcode/releases/download/#{version}/vtcode-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "4d84574e8632ba9cc7cbe3001c156f7b8d83f36a13659c500b3f9666ec8c2379"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/vinhnx/vtcode/releases/download/#{version}/vtcode-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "ab77663ff1efd78bad21ac62a08e8ef885505ea8946895525e958d6e737207b2"
    elsif Hardware::CPU.arm?
      url "https://github.com/vinhnx/vtcode/releases/download/#{version}/vtcode-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "555c798e8c2d671724b7a3bd235a413a3c2c8f33cc4a2b78b4d55a1af0dc9a91"
    else
      odie "VT Code #{version} does not support this Linux architecture"
    end
  end

  def install
    bin.install "vtcode"
  end

  def caveats
    <<~EOS
      VT Code is now installed! To get started:

      1. Set your API key environment variable:
         export OPENAI_API_KEY="sk-..."
         (or use ANTHROPIC_API_KEY, GEMINI_API_KEY, etc.)

      2. Launch VT Code:
         vtcode

      Supported providers:
        • OpenAI (OPENAI_API_KEY)
        • Anthropic (ANTHROPIC_API_KEY)
        • Google Gemini (GEMINI_API_KEY)
        • xAI (XAI_API_KEY)
        • DeepSeek (DEEPSEEK_API_KEY)
        • OpenRouter (OPENROUTER_API_KEY)
        • Ollama (local)

      For more information, visit:
        https://github.com/vinhnx/vtcode
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/vtcode --version")
  end
end
