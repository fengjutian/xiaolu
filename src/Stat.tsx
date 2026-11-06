import type { ElementType } from "react";
import { Box, Card, Group, Text, ThemeIcon, Title } from "@mantine/core";

export function Stat({
  icon: Icon,
  label,
  value,
  hint,
  color,
}: {
  icon: ElementType;
  label: string;
  value: string;
  hint: string;
  color: string;
}) {
  return (
    <Card className="stat-card" padding="lg" radius="lg">
      <Group justify="space-between" align="flex-start">
        <Box>
          <Text size="xs" c="dimmed" fw={700}>
            {label}
          </Text>
          <Title order={2} mt={6}>
            {value}
          </Title>
          <Text size="xs" c="dimmed" mt={6}>
            {hint}
          </Text>
        </Box>
        <ThemeIcon size={42} radius="xl" variant="light" color={color}>
          <Icon size={22} />
        </ThemeIcon>
      </Group>
    </Card>
  );
}
