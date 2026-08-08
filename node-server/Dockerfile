FROM node:22-alpine

WORKDIR /app

COPY react-app/package.json react-app/package-lock.json ./
RUN npm ci

COPY react-app/ ./

EXPOSE 3000

CMD ["npx", "vite", "--host", "0.0.0.0", "--port", "3000"]
