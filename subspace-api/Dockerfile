FROM oven/bun:1.4.4-debian@sha256:93d5032fe06d163427822dc1598f420363759bb18155c9cc90eb1be05a64a19e AS dependencies
WORKDIR /app
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile --production

FROM oven/bun:1.4.4-debian@sha256:93d5032fe06d163427822dc1598f420363759bb18155c9cc90eb1be05a64a19e
WORKDIR /app
COPY --from=dependencies /app/node_modules ./node_modules
COPY package.json ./
COPY src ./src
COPY scripts ./scripts
COPY drizzle ./drizzle
RUN install -d -o bun -g bun -m 0700 /app/.data
USER bun
ENV HOST=0.0.0.0 NODE_ENV=production
EXPOSE 3000
HEALTHCHECK --interval=5s --timeout=3s --start-period=10s \
  CMD ["bun", "-e", "const r = await fetch('http://127.0.0.1:3000/health'); if (!r.ok) process.exit(1);"]
CMD ["sh", "-c", "bun run db:migrate && exec bun src/main.ts"]
