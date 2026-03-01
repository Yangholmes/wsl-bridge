# Design Tokens 模板与规范

## 使用说明

- 在创建设计系统、主题变量、组件样式基线时使用本模板。
- 先定义语义 Token，再映射到组件层 Token，最后映射到具体实现层。
- 避免直接在组件里写硬编码值（颜色、间距、圆角、阴影）。

## 命名规范

- 采用 `类别.语义.状态` 结构，例如 `color.text.primary`。
- 语义命名优先，避免 `blue-500` 这类纯色值命名暴露实现。
- 深浅主题共用语义名称，仅替换底层值。

## 建议层级

1. 原始值层（raw/base）
2. 语义层（semantic）
3. 组件层（component）

## JSON 示例（Style Dictionary 友好）

```json
{
  "color": {
    "base": {
      "white": { "value": "#FFFFFF" },
      "black": { "value": "#111111" },
      "blue-600": { "value": "#0B5FFF" },
      "gray-100": { "value": "#F5F7FA" },
      "gray-700": { "value": "#344054" }
    },
    "text": {
      "primary": { "value": "{color.base.black.value}" },
      "secondary": { "value": "{color.base.gray-700.value}" },
      "inverse": { "value": "{color.base.white.value}" }
    },
    "bg": {
      "canvas": { "value": "{color.base.white.value}" },
      "subtle": { "value": "{color.base.gray-100.value}" },
      "brand": { "value": "{color.base.blue-600.value}" }
    }
  },
  "spacing": {
    "2": { "value": "0.125rem" },
    "4": { "value": "0.25rem" },
    "8": { "value": "0.5rem" },
    "12": { "value": "0.75rem" },
    "16": { "value": "1rem" },
    "24": { "value": "1.5rem" },
    "32": { "value": "2rem" }
  },
  "radius": {
    "sm": { "value": "0.25rem" },
    "md": { "value": "0.5rem" },
    "lg": { "value": "0.75rem" },
    "pill": { "value": "999px" }
  },
  "shadow": {
    "sm": { "value": "0 1px 2px rgba(16,24,40,0.08)" },
    "md": { "value": "0 4px 12px rgba(16,24,40,0.12)" }
  },
  "font": {
    "family": {
      "sans": { "value": "'Noto Sans SC', 'PingFang SC', sans-serif" }
    },
    "size": {
      "12": { "value": "0.75rem" },
      "14": { "value": "0.875rem" },
      "16": { "value": "1rem" },
      "20": { "value": "1.25rem" },
      "24": { "value": "1.5rem" }
    },
    "lineHeight": {
      "tight": { "value": "1.2" },
      "normal": { "value": "1.5" },
      "loose": { "value": "1.7" }
    }
  }
}
```

## CSS 变量示例

```css
:root {
  --color-text-primary: #111111;
  --color-text-secondary: #344054;
  --color-bg-canvas: #ffffff;
  --color-bg-brand: #0b5fff;
  --space-8: 0.5rem;
  --space-16: 1rem;
  --radius-md: 0.5rem;
  --shadow-sm: 0 1px 2px rgba(16, 24, 40, 0.08);
}
```

## 组件映射示例

```txt
Button/Primary
- 背景: color.bg.brand
- 文字: color.text.inverse
- 圆角: radius.md
- 内边距: spacing.12 + spacing.16
- 阴影: shadow.sm
```

## 交付检查

- Token 命名是否语义化且可扩展。
- 亮色/暗色主题是否使用同一语义键。
- 是否避免组件内硬编码样式值。
- 是否提供从设计到代码的一致映射关系。
