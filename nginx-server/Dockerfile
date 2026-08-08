FROM nginx:alpine-slim

COPY --chown=nginx:nginx nginx-server/default.conf /etc/nginx/conf.d/default.conf
COPY --chown=nginx:nginx public/ /usr/share/nginx/html/

EXPOSE 3000

CMD ["nginx", "-g", "daemon off;"]
