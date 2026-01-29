"""
API 兼容性测试

测试用例：
1. Anthropic API - 不带工具
2. Anthropic API - 带工具
3. OpenAI API - 不带工具
4. OpenAI API - 带工具
"""

import asyncio
import unittest
from anthropic import AsyncAnthropic
from openai import AsyncOpenAI


# 配置
BASE_URL = "http://localhost:8990"
API_KEY = "sk-kiro-rs-dasoifoiasx"

# 工具定义
ANTHROPIC_TOOLS = [
    {
        "name": "get_current_weather",
        "description": "Get the current weather in a given location",
        "input_schema": {
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and state, e.g. San Francisco, CA"
                },
                "unit": {
                    "type": "string",
                    "enum": ["celsius", "fahrenheit"]
                }
            },
            "required": ["location"]
        }
    }
]

OPENAI_TOOLS = [
    {
        "type": "function",
        "function": {
            "name": "get_current_weather",
            "description": "Get the current weather in a given location",
            "parameters": {
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA"
                    },
                    "unit": {
                        "type": "string",
                        "enum": ["celsius", "fahrenheit"]
                    }
                },
                "required": ["location"]
            }
        }
    }
]


class TestAnthropicAPI(unittest.TestCase):
    """Anthropic API 测试"""

    def setUp(self):
        self.client = AsyncAnthropic(
            api_key=API_KEY,
            base_url=BASE_URL  # Anthropic SDK 会自动添加 /v1
        )

    def test_01_anthropic_without_tools(self):
        """测试 Anthropic API - 不带工具"""
        async def run_test():
            print("\n=== Test: Anthropic API without tools ===")

            stream = await self.client.messages.create(
                model="claude-sonnet-4.5",
                max_tokens=1024,
                messages=[
                    {"role": "user", "content": "说一句简短的问候语"}
                ],
                stream=True
            )

            full_content = ""
            async for event in stream:
                if event.type == "content_block_delta":
                    if hasattr(event.delta, "text"):
                        full_content += event.delta.text
                        print(f"  Text: {event.delta.text}")
                elif event.type == "message_delta":
                    print(f"  Stop reason: {event.delta.stop_reason}")
                    self.assertEqual(event.delta.stop_reason, "end_turn")

            print(f"\nFull content: {full_content}")
            self.assertTrue(len(full_content) > 0, "Should have content")
            print("=== PASSED ===\n")

        asyncio.run(run_test())

    def test_02_anthropic_with_tools(self):
        """测试 Anthropic API - 带工具"""
        async def run_test():
            print("\n=== Test: Anthropic API with tools ===")

            stream = await self.client.messages.create(
                model="claude-sonnet-4.5",
                max_tokens=1024,
                messages=[
                    {"role": "user", "content": "查询北京的天气"}
                ],
                tools=ANTHROPIC_TOOLS,
                stream=True
            )

            full_content = ""
            tool_uses = []
            current_tool = None

            async for event in stream:
                if event.type == "content_block_start":
                    if hasattr(event.content_block, "type"):
                        if event.content_block.type == "tool_use":
                            current_tool = {
                                "id": event.content_block.id,
                                "name": event.content_block.name,
                                "input": ""
                            }
                            print(f"  Tool start: {event.content_block.name}")
                elif event.type == "content_block_delta":
                    if hasattr(event.delta, "text"):
                        full_content += event.delta.text
                    elif hasattr(event.delta, "partial_json"):
                        if current_tool:
                            current_tool["input"] += event.delta.partial_json
                            print(f"  Tool input delta: {event.delta.partial_json}")
                elif event.type == "content_block_stop":
                    if current_tool:
                        tool_uses.append(current_tool)
                        current_tool = None
                elif event.type == "message_delta":
                    print(f"  Stop reason: {event.delta.stop_reason}")
                    self.assertEqual(event.delta.stop_reason, "tool_use")

            print(f"\nFull content: {full_content}")
            print(f"Tool uses: {tool_uses}")

            self.assertTrue(len(tool_uses) > 0, "Should have tool calls")
            self.assertEqual(tool_uses[0]["name"], "get_current_weather")
            self.assertIn("Beijing", tool_uses[0]["input"])
            print("=== PASSED ===\n")

        asyncio.run(run_test())


class TestOpenAIAPI(unittest.TestCase):
    """OpenAI API 测试"""

    def setUp(self):
        self.client = AsyncOpenAI(
            api_key=API_KEY,
            base_url=f"{BASE_URL}/v1"
        )

    def test_03_openai_without_tools(self):
        """测试 OpenAI API - 不带工具"""
        async def run_test():
            print("\n=== Test: OpenAI API without tools ===")

            stream = await self.client.chat.completions.create(
                model="claude-sonnet-4.5",
                max_tokens=1024,
                messages=[
                    {"role": "user", "content": "说一句简短的问候语"}
                ],
                stream=True
            )

            full_content = ""
            finish_reason = None

            async for chunk in stream:
                if chunk.choices and len(chunk.choices) > 0:
                    delta = chunk.choices[0].delta
                    if delta.content:
                        full_content += delta.content
                        print(f"  Content: {delta.content}")
                    if chunk.choices[0].finish_reason:
                        finish_reason = chunk.choices[0].finish_reason
                        print(f"  Finish reason: {finish_reason}")

            print(f"\nFull content: {full_content}")
            self.assertTrue(len(full_content) > 0, "Should have content")
            self.assertEqual(finish_reason, "stop")
            print("=== PASSED ===\n")

        asyncio.run(run_test())

    def test_04_openai_with_tools(self):
        """测试 OpenAI API - 带工具"""
        async def run_test():
            print("\n=== Test: OpenAI API with tools ===")

            stream = await self.client.chat.completions.create(
                model="claude-sonnet-4.5",
                max_tokens=1024,
                messages=[
                    {"role": "user", "content": "查询北京的天气"}
                ],
                tools=OPENAI_TOOLS,
                tool_choice="auto",
                stream=True
            )

            full_content = ""
            tool_calls = []
            current_tool_call = None
            finish_reason = None

            async for chunk in stream:
                if chunk.choices and len(chunk.choices) > 0:
                    delta = chunk.choices[0].delta

                    if delta.content:
                        full_content += delta.content
                        print(f"  Content: {delta.content}")

                    if delta.tool_calls:
                        for tc in delta.tool_calls:
                            if tc.id:
                                current_tool_call = {
                                    "id": tc.id,
                                    "type": tc.type,
                                    "function": {
                                        "name": tc.function.name if tc.function else "",
                                        "arguments": ""
                                    }
                                }
                                tool_calls.append(current_tool_call)
                                print(f"  Tool call start: {tc.function.name if tc.function else ''}")
                            elif current_tool_call and tc.function and tc.function.arguments:
                                current_tool_call["function"]["arguments"] += tc.function.arguments
                                print(f"  Tool args: {tc.function.arguments}")

                    if chunk.choices[0].finish_reason:
                        finish_reason = chunk.choices[0].finish_reason
                        print(f"  Finish reason: {finish_reason}")

            print(f"\nFull content: {full_content}")
            print(f"Tool calls: {tool_calls}")

            self.assertTrue(len(tool_calls) > 0, "Should have tool calls")
            self.assertEqual(tool_calls[0]["function"]["name"], "get_current_weather")
            self.assertIn("Beijing", tool_calls[0]["function"]["arguments"])
            self.assertEqual(finish_reason, "tool_calls")
            print("=== PASSED ===\n")

        asyncio.run(run_test())


if __name__ == "__main__":
    print("=" * 60)
    print("API Compatibility Tests")
    print("=" * 60)
    print(f"Base URL: {BASE_URL}")
    print(f"API Key: {API_KEY[:20]}...")
    print("=" * 60)

    # 按顺序运行测试
    unittest.main(verbosity=2)
