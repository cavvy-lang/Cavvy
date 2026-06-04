import re

# Read file
with open(r'e:\spj\EOL\src\types.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Pattern: after is_native: false, before is_final/is_override, insert is_abstract: false
# We need to add is_abstract: false to all MethodInfo literals that don't have it

# First, let's find all MethodInfo blocks and add is_abstract if missing
# Pattern to match lines from is_native to is_final/is_override in MethodInfo literals

def add_is_abstract(match):
    block = match.group(0)
    if 'is_abstract' in block:
        return block
    # Insert is_abstract: false after is_native line
    block = re.sub(
        r'(is_native:\s*(true|false),\n)',
        r'\1            is_abstract: false,\n',
        block
    )
    return block

# Match MethodInfo { ... } blocks (non-greedy)
# But we need a line-based approach
pattern = r'(            is_native:\s*(true|false),\n)(            is_(final|override):)'
replacement = r'\1            is_abstract: false,\n\3'

content = re.sub(pattern, replacement, content)

with open(r'e:\spj\EOL\src\types.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("Done patching src/types.rs")
