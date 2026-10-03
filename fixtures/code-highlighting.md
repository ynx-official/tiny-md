# 按语言高亮

## Java

```java
public class Main {
    /* 多行注释
       return 42; 仍然是注释 */
    public static void main(String[] args) {
        String message = "你好，tiny-md！";
        System.out.println(message);
    }
}
```

## Shell

```bash
# 中文注释
message="你好，tiny-md！"
if [ -n "$message" ]; then
    echo "$message"
fi
```

## YAML

~~~yml
# 配置
app:
  name: "tiny-md"
  enabled: true
  port: 8080
~~~

## HTML 与 CSS

```html
<!-- 中文注释 -->
<div class="card">你好，tiny-md！</div>
<script>const count = 42;</script>
```

```css
/* 卡片样式 */
.card {
    color: #336699;
    padding: 16px;
}
```

## Python 多行字符串

```PY example
def greet():
    message = """你好

return 42 仍然是字符串
"""
    return message
```

## TypeScript

```typescript
const greet = (name: string): string => {
    // 中文注释
    return `你好，${name}！`;
};
```

## JSON

```json
{"message": "你好，tiny-md！", "count": 42, "enabled": true}
```

## 未指定或未知语言保持纯文本

```
fn main() { println!("你好"); }
```

```unknown-language
return 42; # 这段内容不应错误染色
```
